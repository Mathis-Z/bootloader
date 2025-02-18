## What is this?

This project is a UEFI bootloader similar to grub, written in Rust. It can boot Linux and modern Windows systems but is focused on Linux. It is meant for demonstration and educational purposes rather than production use.

## Features
- Starting x86_64 Linux bzImages (oldest tested kernel 3.11.0) with both the deprecated EFI handover protocol and the normal [64 bit boot protocol](https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt)
- EFI chainloading (starting other .efi applications like grub or the Windows bootloader)
- Reading from FAT, ext2 and ext4 file systems (The crate for ext2/4 file systems can currently only read them if the journal is empty! Mount and unmount your disk to empty the journal if necessary)

## Missing Features

- Booting OpenBSD / FreeBSD
- Reading CDROMs (missing a crate for parsing [ISO9660](https://en.wikipedia.org/wiki/ISO_9660) unfortunately)
- Support for more file systems
- Advanced features like network booting, secure boot, etc.

## Starting using QEMU

1. Create a virtual drive containing your system(s) using virtualbox
2. Edit the Makefile so your drive gets mounted (make sure the partition with bootloader gets mounted first)
3. Start with `make run`

## Debugging with gdb

- Start with `make debug`
- Connect to `target remote :1234` using gdb ([like this](https://qemu-project.gitlab.io/qemu/system/gdb.html))
- Normal breakpoints don't work; use hardware assisted breakpoints (hbreak) instead
- Use the pwndbg plugin if you want gdb to look cool
- To get output from early kernel booting, set "keep_bootcon earlyprintk=serial,ttyS0,115200" in the kernel cmdline and switch to serial0 in QEMU

## Links

- Rust UEFI github: https://github.com/rust-osdev/uefi-rs
- Rust UEFI tutorial: https://rust-osdev.github.io/uefi-rs/HEAD/introduction.html
- Linux Kernel Boot Protocol: https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt
- Kernel booting process: https://0xax.gitbooks.io/linux-insides/content/Booting/linux-bootstrap-1.html
- EFI handover protocol: https://www.kernel.org/doc/html/v5.6/x86/boot.html#efi-handover-protocol
