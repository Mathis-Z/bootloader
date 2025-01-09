## Features
- Starting (recent) Linux bzImages with both the deprecated EFI handover protocol and the normal [64 bit boot protocol](https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt)
- EFI chain loading (starting other .efi applications)
- Reading from FAT, ext2 and ext4 file systems

## Missing Features

- Booting OpenBSD / FreeBSD
- Reading CDROMs (missing a crate for parsing [ISO9660](https://en.wikipedia.org/wiki/ISO_9660) unfortunately)
- Booting older kernels that aren't relocatable in memory
- Support for more file systems
- Advanced features like network booting, secure boot, etc.

## Starting using QEMU

1. Create a virtual drive containing your system(s) using virtualbox
2. Edit the Makefile so your drive gets mounted (make sure the partition with bootloader gets mounted first)
3. Start with `make run` and hope for the best

## Debugging with gdb

- Start with `make debug`
- Connect to `target remote localhost:1234` using gdb ([like this](https://qemu-project.gitlab.io/qemu/system/gdb.html))
- Normal breakpoints don't work; use hardware assisted breakpoints (hbreak) instead
- Use the pwndbg plugin if you want gdb to look cool
- To get output from early kernel booting, set "keep_bootcon earlyprintk=ttyS0,115200" in the kernel cmdline and switch to serial0 in QEMU

## Links

- Rust UEFI github: https://github.com/rust-osdev/uefi-rs
- Rust UEFI tutorial: https://rust-osdev.github.io/uefi-rs/HEAD/introduction.html
- Linux Kernel Boot Protocol: https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt
- Kernel booting process: https://0xax.gitbooks.io/linux-insides/content/Booting/linux-bootstrap-1.html
- EFI handover protocol: https://www.kernel.org/doc/html/v5.6/x86/boot.html#efi-handover-protocol
