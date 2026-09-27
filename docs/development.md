# NIAT Development Guide

## Prerequisites

- Linux x86_64 host (Fedora, Debian/Ubuntu, Arch)
- Rust toolchain (stable, edition 2024)
- `qemu-system-x86_64` and `edk2-ovmf` for emulation
- `mtools`, `dosfstools`, `e2fsprogs`, `sfdisk` for disk image creation

## Setup

```bash
./scripts/setup.sh
```

## Building the OS

To build all Rust crates, minimal rootfs, and generate bootable disk images (`dist/niat.img` and `dist/niat.iso`):

```bash
./scripts/build.sh
```

## Running in QEMU

Test the built image inside UEFI QEMU emulator:

```bash
./scripts/run-qemu.sh
```

## Flashing to USB

To create a bootable USB drive for physical hardware:

```bash
./scripts/make-usb.sh /dev/sdX
```

## Keybindings in NIAT TUI

- `Enter`: Enter text editing mode / submit intent
- `Esc`: Exit input editing mode
- `F2`: Break out into interactive BusyBox `ash` shell
- `[Y] / [N]`: Approve or Deny pending tool confirmation dialogs
- `Ctrl + L`: Clear workspace history
- `Ctrl + C` or `Ctrl + Q`: Exit TUI
