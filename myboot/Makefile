build:
	cargo build --target x86_64-unknown-uefi


run: build
	cp target/x86_64-unknown-uefi/debug/myboot.efi qemu/x86/esp/efi/boot/bootx64.efi

	qemu-system-x86_64 -m 4G -smp cores=8 \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_CODE.fd \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:qemu/x86/esp \

#		-drive format=vdi,file="/home/mathisz/VirtualBox VMs/ubuntu/test.vdi" \



#		-drive format=vdi,file="/home/mathisz/VirtualBox VMs/ubuntu/ubuntu.vdi" 
#		-drive format=raw,file=fat:rw:qemu/grub \

debug: build
	cp target/x86_64-unknown-uefi/debug/myboot.efi qemu/x86/esp/efi/boot/bootx64.efi

	qemu-system-x86_64 -s -S -m 4G -smp cores=8 -gdb tcp::1234 \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_CODE.fd \
		-drive if=pflash,format=raw,readonly=on,file=qemu/x86/OVMF_VARS.fd \
		-drive format=raw,file=fat:rw:qemu/x86/esp \

#		-drive format=vdi,file="/home/mathisz/VirtualBox VMs/ubuntu/test.vdi" \