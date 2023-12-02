# Migration from rust-osdev/bootloader

This guide summarizes the steps for migrating from `rust-osdev/bootloader` to `springboard`.

## Kernel

- Replace the `bootloader_api` dependency of your kernel with a dependency on the `springboard_api` crate and adjust the import path in your `main.rs`:
  ```diff
   # in Cargo.toml

  -bootloader_api = { version = "0.11" }
  +springboard_api = { git="https://github.com/azyklus/springboard", branch="latest" }
  ```
  ```diff
   // in main.rs

  -use bootloader_api::{entry_point, BootInfo};
  +use springboard_api::{start, BootInfo};
  ```
- If you used optional features, such as `map-physical-memory`, you can enable them again through the `start` macro:
  ```rust
  use springboard_api::config::{BootloaderConfig, Mapping};

  pub static BOOTLOADER_CONFIG: BootloaderConfig = {
      let mut config = BootloaderConfig::new_default();
      config.mappings.physical_memory = Some(Mapping::Dynamic);
      config
  };

  // add a `config` argument to the `entry_point` macro call
  start!(kernel_main, config = &BOOTLOADER_CONFIG);
  ```

  See the `BootloaderConfig` struct for all configuration options.

To build your kernel, run **`cargo build --target x86_64-unknown-none`**. Since the `x86_64-unknown-none` target is a Tier-2 target, there is no need for `bootimage`, `cargo-xbuild`, or `xargo` anymore. Instead, you can run `rustup target add x86_64-unknown-none` to download precompiled versions of the `core` and `alloc` crates. There is no need for custom JSON-based target files anymore.

## Booting

The `springboard v3.0.1` release simplifies the disk image creation. The [`springboard`](https://mbp2.blog/src/@trident) crate now provides simple functions to create bootable disk images from a kernel. The basic idea is to build your kernel first and then invoke a builder function that calls the disk image creation functions of the `springboard` crate.

See our [disk image creation template](../create-disk-image.md) for a detailed explanation of the new build process.
