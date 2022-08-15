#![no_std]
#![no_main]

use byteorder::{ByteOrder, LittleEndian};
use core::{fmt::Write as _, slice};
use disk::AlignedArrayBuffer;
use mbr_nostd::{PartitionTableEntry, PartitionType};

use crate::{
    disk::{AlignedBuffer, Read, Seek, SeekFrom},
    protected_mode::{
        copy_to_protected_mode, enter_protected_mode_and_jump_to_stage_3, enter_unreal_mode,
    },
};

mod dap;
mod disk;
mod fat;
mod protected_mode;
mod screen;

/// We use this partition type to store the second bootloader stage;
const BOOTLOADER_SECOND_STAGE_PARTITION_TYPE: u8 = 0x20;

const STAGE_3_DST: *mut u8 = 0x0010_0000 as *mut u8; // 1MiB (typically 14MiB accessible here)
const KERNEL_DST: *mut u8 = 0x0100_0000 as *mut u8; // 16MiB

extern "C" {
    static _second_stage_end: u8;
}

fn second_stage_end() -> *const u8 {
    unsafe { &_second_stage_end }
}

static mut DISK_BUFFER: AlignedArrayBuffer<0x4000> = AlignedArrayBuffer {
    buffer: [0; 0x4000],
};

#[no_mangle]
#[link_section = ".start"]
pub extern "C" fn _start(disk_number: u16, partition_table_start: *const u8) {
    screen::Writer.write_str(" -> SECOND STAGE\n").unwrap();

    enter_unreal_mode();

    // parse partition table
    let partitions = {
        const MAX_ENTRIES: usize = 4;
        const ENTRY_SIZE: usize = 16;

        let mut entries = [PartitionTableEntry::empty(); MAX_ENTRIES];
        let raw = unsafe { slice::from_raw_parts(partition_table_start, ENTRY_SIZE * MAX_ENTRIES) };
        for idx in 0..MAX_ENTRIES {
            let offset = idx * ENTRY_SIZE;
            let partition_type = PartitionType::from_mbr_tag_byte(raw[offset + 4]);
            let lba = LittleEndian::read_u32(&raw[offset + 8..]);
            let len = LittleEndian::read_u32(&raw[offset + 12..]);
            entries[idx] = PartitionTableEntry::new(partition_type, lba, len);
        }
        entries
    };
    // look for second stage partition
    let second_stage_partition_idx = partitions
        .iter()
        .enumerate()
        .find(|(_, e)| {
            e.partition_type == PartitionType::Unknown(BOOTLOADER_SECOND_STAGE_PARTITION_TYPE)
        })
        .unwrap()
        .0;
    let fat_partition = partitions.get(second_stage_partition_idx + 1).unwrap();
    assert!(matches!(
        fat_partition.partition_type,
        PartitionType::Fat12(_) | PartitionType::Fat16(_) | PartitionType::Fat32(_)
    ));

    // load fat partition
    let mut disk = disk::DiskAccess {
        disk_number,
        base_offset: u64::from(fat_partition.logical_block_address) * 512,
        current_offset: 0,
    };

    let mut fs = fat::FileSystem::parse(disk.clone());

    let disk_buffer = unsafe { &mut DISK_BUFFER };

    let stage_3_len = load_file("boot-stage-3", STAGE_3_DST, &mut fs, &mut disk, disk_buffer);
    writeln!(screen::Writer, "stage 3 loaded at {STAGE_3_DST:#p}").unwrap();
    let stage_4_dst = {
        let stage_3_end = STAGE_3_DST.wrapping_add(usize::try_from(stage_3_len).unwrap());
        let align_offset = stage_3_end.align_offset(512);
        stage_3_end.wrapping_add(align_offset)
    };
    load_file("boot-stage-4", stage_4_dst, &mut fs, &mut disk, disk_buffer);
    writeln!(screen::Writer, "stage 4 loaded at {stage_4_dst:#p}").unwrap();
    load_file("kernel-x86_64", KERNEL_DST, &mut fs, &mut disk, disk_buffer);
    writeln!(screen::Writer, "kernel loaded at {KERNEL_DST:#p}").unwrap();

    // TODO: Retrieve memory map
    // TODO: VESA config

    enter_protected_mode_and_jump_to_stage_3(STAGE_3_DST);

    loop {}
}

fn load_file(
    file_name: &str,
    dst: *mut u8,
    fs: &mut fat::FileSystem<disk::DiskAccess>,
    disk: &mut disk::DiskAccess,
    disk_buffer: &mut AlignedArrayBuffer<16384>,
) -> u64 {
    let disk_buffer_size = disk_buffer.buffer.len();
    let kernel = fs
        .find_file_in_root_dir(file_name, disk_buffer)
        .expect("file not found");
    let mut total_size = 0;
    for cluster in fs.file_clusters(&kernel) {
        let cluster = cluster.unwrap();
        let cluster_start = cluster.start_offset;
        let cluster_end = cluster_start + u64::from(cluster.len_bytes);
        total_size += u64::from(cluster.len_bytes);

        let mut offset = 0;
        loop {
            let range_start = cluster_start + offset;
            if range_start >= cluster_end {
                break;
            }
            let range_end = u64::min(
                range_start + u64::try_from(disk_buffer_size).unwrap(),
                cluster_end,
            );
            let len = range_end - range_start;

            writeln!(
                screen::Writer,
                "loading bytes {range_start:#x}..{range_end:#x}"
            )
            .unwrap();

            disk.seek(SeekFrom::Start(range_start));
            disk.read_exact_into(disk_buffer_size, disk_buffer);

            let slice = &disk_buffer.buffer[..usize::try_from(len).unwrap()];
            unsafe {
                copy_to_protected_mode(dst.wrapping_add(usize::try_from(offset).unwrap()), slice)
            };
            let written = unsafe {
                protected_mode::read_from_protected_mode(
                    dst.wrapping_add(usize::try_from(offset).unwrap()),
                )
            };
            assert_eq!(slice[0], written);

            offset += len;
        }
    }
    total_size
}

/// Taken from https://github.com/rust-lang/rust/blob/e100ec5bc7cd768ec17d75448b29c9ab4a39272b/library/core/src/slice/mod.rs#L1673-L1677
///
/// TODO replace with `split_array` feature in stdlib as soon as it's stabilized,
/// see https://github.com/rust-lang/rust/issues/90091
fn split_array_ref<const N: usize, T>(slice: &[T]) -> (&[T; N], &[T]) {
    if N > slice.len() {
        fail(b'S');
    }
    let (a, b) = slice.split_at(N);
    // SAFETY: a points to [T; N]? Yes it's [T] of length N (checked by split_at)
    unsafe { (&*(a.as_ptr() as *const [T; N]), b) }
}

#[cold]
#[inline(never)]
#[no_mangle]
pub extern "C" fn fail(code: u8) -> ! {
    panic!("fail: {}", code as char);
}
