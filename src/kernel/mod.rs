extern crate alloc;

mod params;

use core::arch::asm;

use alloc::vec::Vec;
use uefi::boot::MemoryType;
use uefi::println;

use crate::simple_error::{simple_error, SimpleResult};

use self::params::*;

pub struct Kernel {
    blob: Vec<u8>,
    kernel_params: KernelParams,
}

impl Kernel {
    pub fn new(image: Vec<u8>) -> SimpleResult<Self> {
        let kernel = Kernel {
            kernel_params: KernelParams::new(&image),
            blob: image,
        };

        if !kernel.check_magic_number() {
            return simple_error!("Kernel image does not start with magic bytes 'MZ'!");
        }

        Ok(kernel)
    }

    pub fn check_magic_number(&self) -> bool {
        return self.blob[0] == 0x4d && self.blob[1] == 0x5a;
    }

    fn start_efi_handover(&mut self) -> ! {
        println!("Starting using efi handover");

        let protected_mode_kernel_addr = self.extract_protected_mode_kernel_to_aligned_address();

        // calculating entry point
        let handover_offset = self.kernel_params.get_param(KernelParam::HandoverOffset);
        let entry_point_efi_64bit = protected_mode_kernel_addr + 0x200 + handover_offset;

        // zero page has space for other parameters (that we do not have to set with the EFI handover protocol)
        let zero_page_addr = self.kernel_params.copy_into_zero_page();

        println!("Entering kernel, bye...");

        Kernel::jump_to_efi_entry(
            entry_point_efi_64bit,
            uefi::boot::image_handle().as_ptr() as usize,
            uefi::table::system_table_raw().unwrap().as_ptr() as usize,
            zero_page_addr,
        );
    }

    fn extract_protected_mode_kernel_to_aligned_address(&mut self) -> usize {
        // copying protected mode kernel to aligned address (actually the alignment should be 2M but this still works)
        let setup_code_size = self.kernel_params.get_param(KernelParam::SetupSects) * 512;
        let protected_mode_kernel_start = (setup_code_size + 512).try_into().unwrap();

        let addr =
            crate::mem::copy_buf_to_aligned_address(&self.blob[protected_mode_kernel_start..]);

        // "if a bootloader which does not install a hook loads a relocatable kernel at a nonstandard address it will have to modify this field to point to the load address."
        self.kernel_params.set_param(KernelParam::Code32Start, addr);

        println!("protected-mode kernel code copied to {:x}", addr);

        addr
    }

    fn set_cmdline(&mut self, cmdline: &str) {
        let cmdline_addr = allocate_cmdline(cmdline);
        println!("Cmdline allocated at {:x}", cmdline_addr);
        self.kernel_params
            .set_param(KernelParam::CmdLinePtr, cmdline_addr);
    }

    fn set_ramdisk(&mut self, ramdisk: Option<Vec<u8>>) {
        if let Some(ramdisk) = ramdisk {
            self.kernel_params.set_param(KernelParam::RamdiskImage, ramdisk.as_ptr() as usize);
            self.kernel_params.set_param(KernelParam::RamdiskSize, ramdisk.len() as usize);
        } else {
            self.kernel_params.set_param(KernelParam::RamdiskImage, 0);
            self.kernel_params.set_param(KernelParam::RamdiskSize, 0);
        }
    }

    fn setup_stack_and_heap(&mut self) -> usize {
        let stack_top_addr = crate::mem::allocate_low_pages(8) + 8 * 4096;
        let heap_end_addr = crate::mem::allocate_low_pages(8) + 8 * 4096;

        println!(
            "Stack top is at {:x}, Heap end is at {:x}",
            stack_top_addr, heap_end_addr
        );

        self.kernel_params
            .set_param(KernelParam::HeapEndPtr, heap_end_addr);

        stack_top_addr // stack pointer is passed in rsp register
    }

    fn start_normal_handover(&mut self) -> ! {
        println!("Starting using normal handover");

        let _low_pages_for_kernel = crate::mem::allocate_low_pages(1000); // TODO: check if necessary

        let protected_mode_kernel_addr = self.extract_protected_mode_kernel_to_aligned_address();
        let stack_top_addr = self.setup_stack_and_heap();

        let entry_point = protected_mode_kernel_addr + 0x200; // 64bit entry point is at +0x200 of protected-mode code
        println!("Entry point is at {:x}", entry_point);

        let zero_page_addr = self.kernel_params.copy_into_zero_page();

        println!(
            "kernel params copied into zero page at {:x}",
            zero_page_addr
        );

        let gdt_page = crate::mem::gdt::allocate_page_for_gdt();
        println!("Building page tables...");
        let pml4_ptr = unsafe { crate::mem::paging::prepare_identity_mapped_pml4() } as usize;
        println!("Page tables built at: {:x}", pml4_ptr);

        println!("Exiting boot services. Goodbye");

        unsafe {
            let old_mmap = uefi::boot::exit_boot_services(MemoryType::LOADER_DATA);

            KernelParams::set_memory_map(zero_page_addr, &old_mmap);
            crate::mem::gdt::create_and_set_simple_gdt(gdt_page);

            Kernel::run(pml4_ptr, stack_top_addr, entry_point, zero_page_addr);
        }
    }

    pub fn start(&mut self, cmdline: &str, ramdisk: Option<Vec<u8>>) {
        if self.kernel_params.get_param(KernelParam::RelocatableKernel) == 0 {
            println!("Kernel is not relocatable! This code only works for relocatable kernels :(");
            return;
        }

        // setting parameters shared by both handover methods
        self.kernel_params
            .set_param(KernelParam::TypeOfLoader, 0xFF); // custom bootloader
        self.set_ramdisk(ramdisk);
        self.set_cmdline(cmdline);

        if self.kernel_params.get_param(KernelParam::XLoadflags) & 0b100 == 0 {
            self.start_normal_handover();
        } else {
            self.start_efi_handover();
        }
    }

    fn jump_to_efi_entry(entry_point: usize, handle: usize, system_table: usize, boot_params: usize) -> ! {
        unsafe {
            asm!(
                r#"
                jmp {}
                "#,
                in(reg) entry_point,
                in("rdi") handle,
                in("rsi") system_table,
                in("rdx") boot_params,
            );
        }
        unreachable!();
    }

    // https://github.com/rust-osdev/bootloader/blob/main/common/src/lib.rs
    unsafe fn run(page_table_addr: usize, stack_top: usize, entry_point: usize, boot_info: usize) -> ! {
        unsafe {
            asm!(
                r#"
                xor rbp, rbp
                mov cr3, {}
                mov rsp, {}
                push 0
                jmp {}
                "#,
                in(reg) page_table_addr,
                in(reg) stack_top,
                in(reg) entry_point,
                in("rsi") boot_info,
            );
        }
        unreachable!();
    }
}
