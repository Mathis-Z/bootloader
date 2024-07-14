Rust UEFI github: https://github.com/rust-osdev/uefi-rs
Rust-UEFI tutorial: https://rust-osdev.github.io/uefi-rs/HEAD/introduction.html

Das ganze läuft jetzt mit QEMU. Ich habe ein Makefile geschrieben, mit dem du einfach ```make run_aarch64``` ausführen kannst (können solltest, wenn alles klappt, was ja eher unwahrscheinlich ist...) um das ganze zu bauen und QEMU zu starten.


Linux Kernel Boot Protocol: https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt

https://0xax.gitbooks.io/linux-insides/content/Booting/linux-bootstrap-1.html

Wichtige Quelle:

https://www.kernel.org/doc/html/v5.6/x86/boot.html#efi-handover-protocol

https://www.lse.epita.fr/lse-summer-week-2015/slides/lse-summer-week-2015-05-Linux_Boot_Protocol.pdf