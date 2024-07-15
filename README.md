## Features
- EFI chain loading (andere EFI Apps starten)
- FAT-Dateisysteme traversieren
- Linux bzImage mit EFI handover protocol starten (das ist eigentlich deprecated, auf dem experimental-branch gibt es einen Ansatz, der das "normale" 64bit boot protocol versucht, aber in einer kernel panic endet; Trigger Warnung: chaotischer Code)

## Starten

Wenn man das Makefile so angepasst hat, dass die drives, die QEMU gegeben werden, exisitieren, dann wird das Projekt kompiliert und gestartet mit `make run` (im myboot Ordner). Es kann sein, dass dann nicht die kompilierte EFI-App gestartet wird, sondern GRUB oder was sonst so auf der virtuellen Festplatte unter /EFI/BOOT/BOOTX64.EFI installiert ist. Wenn das passiert, muss man die .EFI austauschen. Um das für VirtualBox-Images etwas schneller zu machen, gibt es ein provisorisches Shellskript (efi.sh).



- `ls` ohne in einem Dateisystem zu sein, listet die verfügbaren Dateisysteme auf. Da wo keine Namen verfügbar sind, werden als Namen die Adressen der Handles benutzt.
- `cd` verhält sich in etwa wie das normale bash-cd
- `runefi` startet eine EFI-App und bekommt als ersten Parameter den Pfad zur .EFI-Datei (Der Pfad muss anfangen mit dem Namen des Dateisystems)
- `runkernel` bekommt als ersten Parameter den Pfad zu einem bzImage und als zweiten Parameter die Kernel-Cmdline (dabei muss zum Beispiel "root=/dev/sdb3" o.ä. gesetzt sein)


## Debuggen

Um mit GDB zu debuggen kann man das Projekt mit `make debug` starten und sich dann in gdb mit `target remote localhost:1234` verbinden (siehe https://qemu-project.gitlab.io/qemu/system/gdb.html).
Normale breakpoints funktionieren aus irgendeinem Grund nicht, aber hardware breakpoints (hbreak) schon.
(Ein schönes GDB-Plugin ist übrigens pwndbg)

Um Output vom Kernel zu bekommen, wenn der Screen noch nicht funktioniert, kann man der Kernel-Commandline die Parameter "keep_bootcon earlyprintk=ttyS0,115200" hinzufügen und in QEMU dann auf serial0 statt VGA umstellen (so sieht man auf dem experimental-branch auch die kernel-panic).


## Quellen

 - Rust UEFI github: https://github.com/rust-osdev/uefi-rs
- Rust-UEFI tutorial: https://rust-osdev.github.io/uefi-rs/HEAD/introduction.html

- Linux Kernel Boot Protocol: https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt
- Kernel booting process: https://0xax.gitbooks.io/linux-insides/content/Booting/linux-bootstrap-1.html

- EFI handover protocol: https://www.kernel.org/doc/html/v5.6/x86/boot.html#efi-handover-protocol

- Foliensatz zu Linux boot protocol: https://www.lse.epita.fr/lse-summer-week-2015/slides/lse-summer-week-2015-05-Linux_Boot_Protocol.pdf

- OSDev wiki für PageTables, GDT, ...
