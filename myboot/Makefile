build_x86:
	cargo build --target x86_64-unknown-uefi

build_aarch64:
	cargo build --target aarch64-unknown-uefi

run_x86: build_x86
	cp target/x86_64-unknown-uefi/debug/myboot.efi qemu/x86/esp/efi/boot/bootx64.efi

	qemu-system-x86_64 -enable-kvm \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_CODE.fd \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:qemu/x86/esp


run_aarch64: build_aarch64
	cp target/aarch64-unknown-uefi/debug/myboot.efi qemu/aarch64/esp/efi/boot/bootaa64.efi

	qemu-system-aarch64 -enable-kvm \
		-drive if=pflash,format=raw,readonly=on,file=qemu/aarch64/OVMF_CODE.fd \
		-drive if=pflash,format=raw,readonly=on,file=qemu/aarch64/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:qemu/aarch64/esp
