extern crate alloc;

use core::arch::asm;

use alloc::vec::Vec;
use uefi::boot::MemoryType;
use uefi::{fs::Path, prelude::*, println, CString16};

use crate::{gdt::*, kernel_params, memory::*, paging::*};

use kernel_params::*;

pub struct Kernel {
    blob: Vec<u8>,
    kernel_params: KernelParams,
}

impl Kernel {
    pub fn load_and_start(cmdline: &CString16, fs_handle: &Handle, path: &CString16) {
        if let Some(mut kernel) = Kernel::load_from(fs_handle, path) {
            if !kernel.check_magic_number() {
                println!("Kernel image does not start with MZ magic number!");
            }

            kernel.start(cmdline)
        } else {
            println!("Kernel not loaded");
        }
    }

    pub fn check_magic_number(&self) -> bool {
        return self.blob[0] == 0x4d || self.blob[1] == 0x5a;
    }

    pub fn load_from(fs_handle: &Handle, path: &CString16) -> Option<Kernel> {
        println!("Loading kernel...");

        let Some(mut fs) = crate::disk_helpers::open_fs_handle(fs_handle) else {
            return None;
        };

        match fs.read(Path::new(path)) {
            Ok(data) => Some(Kernel {
                kernel_params: KernelParams::new(&data),
                blob: data,
            }),
            Err(err) => {
                println!("Error reading vmlinuz image: {}", err);
                None
            }
        }
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
            uefi::boot::image_handle().as_ptr() as u64,
            uefi::table::system_table_raw().unwrap().as_ptr() as u64,
            zero_page_addr,
        );
    }

    fn extract_protected_mode_kernel_to_aligned_address(&mut self) -> u64 {
        // copying protected mode kernel to aligned address (actually the alignment should be 2M but this still works)
        let setup_code_size = self.kernel_params.get_param(KernelParam::SetupSects) * 512;
        let protected_mode_kernel_start: usize = (setup_code_size + 512).try_into().unwrap();

        let addr =
            crate::memory::copy_buf_to_aligned_address(&self.blob[protected_mode_kernel_start..]);

        // "if a bootloader which does not install a hook loads a relocatable kernel at a nonstandard address it will have to modify this field to point to the load address."
        self.kernel_params.set_param(KernelParam::Code32Start, addr);

        println!("protected-mode kernel code copied to {:x}", addr);

        addr
    }

    fn set_cmdline(&mut self, cmdline: &CString16) {
        let cmdline_addr = allocate_cmdline(cmdline);
        println!("Cmdline allocated at {:x}", cmdline_addr);
        self.kernel_params
            .set_param(KernelParam::CmdLinePtr, cmdline_addr);
    }

    fn setup_stack_and_heap(&mut self) -> u64 {
        let stack_top_addr = allocate_low_pages(8) + 8 * 4096;
        let heap_end_addr = allocate_low_pages(8) + 8 * 4096;

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

        let _low_pages_for_kernel = allocate_low_pages(1000); // TODO: check if necessary

        let protected_mode_kernel_addr = self.extract_protected_mode_kernel_to_aligned_address();
        let stack_top_addr = self.setup_stack_and_heap();

        let entry_point = protected_mode_kernel_addr + 0x200; // 64bit entry point is at +0x200 of protected-mode code
        println!("Entry point is at {:x}", entry_point);

        let zero_page_addr = self.kernel_params.copy_into_zero_page();

        println!(
            "kernel params copied into zero page at {:x}",
            zero_page_addr
        );

        let gdt_page = allocate_page_for_gdt();
        println!("Building page tables...");
        let pml4_ptr = unsafe { prepare_identity_mapped_pml4() };
        println!("Page tables built at: {:x}", pml4_ptr as u64);

        println!("Exiting boot services. Goodbye");

        unsafe {
            let old_mmap = uefi::boot::exit_boot_services(MemoryType::LOADER_DATA);

            KernelParams::set_memory_map(zero_page_addr, &old_mmap);
            create_and_set_simple_gdt(gdt_page);

            Kernel::run(pml4_ptr as u64, stack_top_addr, entry_point, zero_page_addr);
        }
    }

    pub fn start(&mut self, cmdline: &CString16) {
        if self.kernel_params.get_param(KernelParam::RelocatableKernel) == 0 {
            println!("Kernel is not relocatable! This code only works for relocatable kernels :(");
            return;
        }

        self.kernel_params
            .set_param(KernelParam::TypeOfLoader, 0xFF); // custom bootloader
        self.kernel_params.set_param(KernelParam::RamdiskImage, 0); // no ramdisk
        self.kernel_params.set_param(KernelParam::RamdiskSize, 0);

        self.set_cmdline(cmdline);

        if self.kernel_params.get_param(KernelParam::XLoadflags) & 0b100 == 0 {
            self.start_normal_handover();
        } else {
            self.start_efi_handover();
        }
    }

    fn jump_to_efi_entry(entry_point: u64, handle: u64, system_table: u64, boot_params: u64) -> ! {
        unsafe {
            asm!(
                r#"
                jmp {}
                "#,
                in(reg) entry_point,
                in("rdi") handle as *const u64 as usize,
                in("rsi") system_table as *const u64 as usize,
                in("rdx") boot_params as *const u64 as usize,
            );
        }
        unreachable!();
    }

    // https://github.com/rust-osdev/bootloader/blob/main/common/src/lib.rs
    unsafe fn run(page_table_addr: u64, stack_top: u64, entry_point: u64, boot_info: u64) -> ! {
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
                in("rsi") boot_info as *const u64 as usize,
            );
        }
        unreachable!();
    }
}
