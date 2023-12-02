#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

use springboard_api::{start, BootInfo};
use test_kernel_ramdisk::{exit_qemu, QemuExitCode};

start!(kernel_main);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    exit_qemu(QemuExitCode::Success);
}

/// This function is called on panic.
#[panic_handler]
#[cfg(not(test))]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    let _ = writeln!(test_kernel_ramdisk::serial(), "PANIC: {info}");
    exit_qemu(QemuExitCode::Failed);
}
