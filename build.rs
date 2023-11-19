#![allow(non_snake_case)]

use async_process::Command;
use futures::executor::block_on;
use futures_concurrency::future::Join;
use std::path::{Path, PathBuf};
const BOOTLOADER_VERSION: &str = env!("CARGO_PKG_VERSION");
const BOOTLOADER_REPO: &str = "https://github.com/azyklus/springboard";

fn main() {
    #[cfg(not(feature = "uefi"))]
    async fn uefiMain() {}
    #[cfg(not(feature = "bios"))]
    async fn biosMain() {}

    block_on((uefiMain(), biosMain()).join());
}

#[cfg(feature = "bios")]
async fn biosMain() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // Run the bios build commands concurrently.
    // (Cargo already uses multiple threads for building dependencies, but these
    // BIOS crates don't have enough dependencies to utilize all cores on modern
    // CPUs. So by running the build commands in parallel, we increase the number
    // of utilized cores.)
    #[cfg(not(docsrs_dummy_build))]
       let (bios_boot_sector_path, bios_stage_2_path, bios_stage_3_path, bios_stage_4_path) = (
        buildBiosBootSector(&out_dir),
        buildBiosStage2(&out_dir),
        buildBiosStage3(&out_dir),
        buildBiosStage4(&out_dir),
    )
       .join()
       .await;
    // dummy implementations because docsrs builds have no network access
    #[cfg(docsrs_dummy_build)]
       let (bios_boot_sector_path, bios_stage_2_path, bios_stage_3_path, bios_stage_4_path) = (
        PathBuf::new(),
        PathBuf::new(),
        PathBuf::new(),
        PathBuf::new(),
    );
    println!(
        "cargo:rustc-env=BIOS_BOOT_SECTOR_PATH={}",
        bios_boot_sector_path.display()
    );
    println!(
        "cargo:rustc-env=BIOS_STAGE_2_PATH={}",
        bios_stage_2_path.display()
    );
    println!(
        "cargo:rustc-env=BIOS_STAGE_3_PATH={}",
        bios_stage_3_path.display()
    );
    println!(
        "cargo:rustc-env=BIOS_STAGE_4_PATH={}",
        bios_stage_4_path.display()
    );
}

#[cfg(feature = "uefi")]
async fn uefiMain() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    #[cfg(not(docsrs_dummy_build))]
       let uefi_path = buildUefiBootloader(&out_dir).await;
    // dummy implementation because docsrs builds have no network access
    #[cfg(docsrs_dummy_build)]
       let uefi_path = PathBuf::new();

    println!(
        "cargo:rustc-env=UEFI_BOOTLOADER_PATH={}",
        uefi_path.display()
    );
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "uefi")]
async fn buildUefiBootloader(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("install").arg("springboard-x86_64-uefi");
    if Path::new("uefi").exists() {
        // local build
        cmd.arg("--path").arg("uefi");
        println!("cargo:rerun-if-changed=uefi");
        println!("cargo:rerun-if-changed=common");
    } else {
        cmd.arg("--git").arg(BOOTLOADER_REPO);
    }
    cmd.arg("--locked");
    cmd.arg("--target").arg("x86_64-unknown-uefi");
    cmd.arg("-Zbuild-std=core")
       .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--root").arg(out_dir);
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    let status = cmd
       .status()
       .await
       .expect("failed to run cargo install for uefi bootloader");
    if status.success() {
        let path = out_dir.join("bin").join("springboard-x86_64-uefi.efi");
        assert!(
            path.exists(),
            "uefi bootloader executable does not exist after building"
        );
        path
    } else {
        panic!("failed to build uefi bootloader");
    }
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "bios")]
async fn buildBiosBootSector(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("install").arg("springboard-x86_64-bios-boot-sector");
    let local_path = Path::new(env!("CARGO_MANIFEST_DIR"))
       .join("bios")
       .join("boot_sector");
    if local_path.exists() {
        // local build
        cmd.arg("--path").arg(&local_path);
        println!("cargo:rerun-if-changed={}", local_path.display());
    } else {
        cmd.arg("--git").arg(BOOTLOADER_REPO);
    }
    cmd.arg("--locked");
    cmd.arg("--target").arg("i386-code16-boot-sector.json");
    cmd.arg("--profile").arg("stage-1");
    cmd.arg("-Zbuild-std=core")
       .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--root").arg(out_dir);
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("RUSTC_WORKSPACE_WRAPPER"); // used by clippy
    let status = cmd
       .status()
       .await
       .expect("failed to run cargo install for bios bootsector");
    let elf_path = if status.success() {
        let path = out_dir
           .join("bin")
           .join("springboard-x86_64-bios-boot-sector");
        assert!(
            path.exists(),
            "bios boot sector executable does not exist after building"
        );
        path
    } else {
        panic!("failed to build bios boot sector");
    };
    convertElfBin(elf_path).await
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "bios")]
async fn buildBiosStage2(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("install").arg("springboard-x86_64-bios-stage-2");
    let local_path = Path::new(env!("CARGO_MANIFEST_DIR"))
       .join("bios")
       .join("stage-2");
    if local_path.exists() {
        // local build
        cmd.arg("--path").arg(&local_path);
        println!("cargo:rerun-if-changed={}", local_path.display());
        println!(
            "cargo:rerun-if-changed={}",
            local_path.with_file_name("common").display()
        );
    } else {
        cmd.arg("--git").arg(BOOTLOADER_REPO);
    }
    cmd.arg("--locked");
    cmd.arg("--target").arg("i386-code16-stage-2.json");
    cmd.arg("--profile").arg("stage-2");
    cmd.arg("-Zbuild-std=core")
       .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--root").arg(out_dir);
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("RUSTC_WORKSPACE_WRAPPER"); // used by clippy
    let status = cmd
       .status()
       .await
       .expect("failed to run cargo install for bios second stage");
    let elf_path = if status.success() {
        let path = out_dir.join("bin").join("springboard-x86_64-bios-stage-2");
        assert!(
            path.exists(),
            "bios second stage executable does not exist after building"
        );
        path
    } else {
        panic!("failed to build bios second stage");
    };
    convertElfBin(elf_path).await
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "bios")]
async fn buildBiosStage3(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("install").arg("springboard-x86_64-bios-stage-3");
    let local_path = Path::new(env!("CARGO_MANIFEST_DIR"))
       .join("bios")
       .join("stage-3");
    if local_path.exists() {
        // local build
        cmd.arg("--path").arg(&local_path);
        println!("cargo:rerun-if-changed={}", local_path.display());
    } else {
        cmd.arg("--git").arg(BOOTLOADER_REPO);
    }
    cmd.arg("--locked");
    cmd.arg("--target").arg("i686-stage-3.json");
    cmd.arg("--profile").arg("stage-3");
    cmd.arg("-Zbuild-std=core")
       .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--root").arg(out_dir);
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("RUSTC_WORKSPACE_WRAPPER"); // used by clippy
    let status = cmd
       .status()
       .await
       .expect("failed to run cargo install for bios stage-3");
    let elf_path = if status.success() {
        let path = out_dir.join("bin").join("springboard-x86_64-bios-stage-3");
        assert!(
            path.exists(),
            "bios stage-3 executable does not exist after building"
        );
        path
    } else {
        panic!("failed to build bios stage-3");
    };
    convertElfBin(elf_path).await
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "bios")]
async fn buildBiosStage4(out_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("install").arg("springboard-x86_64-bios-stage-4");
    let local_path = Path::new(env!("CARGO_MANIFEST_DIR"))
       .join("bios")
       .join("stage-4");
    if local_path.exists() {
        // local build
        cmd.arg("--path").arg(&local_path);
        println!("cargo:rerun-if-changed={}", local_path.display());
    } else {
        cmd.arg("--git").arg(BOOTLOADER_REPO);
    }
    cmd.arg("--locked");
    cmd.arg("--target").arg("x86_64-stage-4.json");
    cmd.arg("--profile").arg("stage-4");
    cmd.arg("-Zbuild-std=core")
       .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--root").arg(out_dir);
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("RUSTC_WORKSPACE_WRAPPER"); // used by clippy
    let status = cmd
       .status()
       .await
       .expect("failed to run cargo install for bios stage-4");
    let elf_path = if status.success() {
        let path = out_dir.join("bin").join("springboard-x86_64-bios-stage-4");
        assert!(
            path.exists(),
            "bios stage-4 executable does not exist after building"
        );
        path
    } else {
        panic!("failed to build bios stage-4");
    };

    convertElfBin(elf_path).await
}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "bios")]
async fn convertElfBin(elf_path: PathBuf) -> PathBuf {
    let flat_binary_path = elf_path.with_extension("bin");

    let llvm_tools = llvm_tools::LlvmTools::new().expect("failed to get llvm tools");
    let objcopy = llvm_tools
       .tool(&llvm_tools::exe("llvm-objcopy"))
       .expect("LlvmObjcopyNotFound");

    // convert first stage to binary
    let mut cmd = Command::new(objcopy);
    cmd.arg("-I").arg("elf64-x86-64");
    cmd.arg("-O").arg("binary");
    cmd.arg("--binary-architecture=i386:x86-64");
    cmd.arg(&elf_path);
    cmd.arg(&flat_binary_path);
    let output = cmd
       .output()
       .await
       .expect("failed to execute llvm-objcopy command");
    if !output.status.success() {
        panic!(
            "objcopy failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    flat_binary_path
}
