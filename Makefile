SYSTEM_DRIVES = -drive format=vdi,file="/home/mathisz/VirtualBox VMs/windows_and_ubuntu/windows_and_ubuntu.vdi"
EFI_DIR = qemu/x86/esp
CARGO_TARGET = x86_64-unknown-uefi

FIRMWARE_DRIVE = \
	-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_CODE.fd \
	-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_VARS.fd \

EFI_DRIVE = -drive format=raw,file=fat:rw:$(EFI_DIR)

DRIVES = $(FIRMWARE_DRIVE) $(EFI_DRIVE) $(SYSTEM_DRIVES)
COMMON_QEMU_SETTINGS = -m 4G -smp cores=8 -vga std

build:
	cargo build --target $(CARGO_TARGET)
	mkdir -p $(EFI_DIR)/efi/boot
	cp target/$(CARGO_TARGET)/debug/myboot.efi $(EFI_DIR)/efi/boot/bootx64.efi

run: build
	qemu-system-x86_64 $(COMMON_QEMU_SETTINGS) $(DRIVES)

debug: build
	qemu-system-x86_64 -d int,pcall,cpu_reset,guest_errors -s -S $(COMMON_QEMU_SETTINGS) $(DRIVES)
