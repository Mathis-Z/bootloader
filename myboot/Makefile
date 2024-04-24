build_x86:
	cargo build --target x86_64-unknown-uefi

build_aarch64:
	cargo build --target aarch64-unknown-uefi

run_x86: build_x86
	cd qemu/x86 && ./run.sh

run_aarch64: build_aarch64
	cd qemu/arm && ./run.sh
