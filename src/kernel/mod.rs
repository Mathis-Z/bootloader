// This file contains the logic to load and start a Linux kernel image.

extern crate alloc;

mod params;

use core::arch::asm;

use alloc::vec::Vec;
use uefi::boot::{self, MemoryType};
use uefi::mem::memory_map::{MemoryMap, MemoryMapOwned};
use uefi::println;
use uefi::proto::console::gop::GraphicsOutput;

use crate::disk::open_protocol_unsafe;
use crate::mem::allocate_low_pages;
use crate::{mem::copy_buf_to_aligned_address, simple_error::{simple_error, SimpleResult}};

use self::params::*;

pub struct Kernel {
    image: Vec<u8>,
}

impl Kernel {
    pub fn new(image: Vec<u8>) -> SimpleResult<Self> {
        let kernel_header = KernelHeader::new(&image)?;

        Kernel::check_support(kernel_header)?;

        let kernel = Kernel {
            image,
        };

        Ok(kernel)
    }

    fn check_support(kernel_header: &KernelHeader) -> SimpleResult<()> {
        if kernel_header.boot_flag != 0xAA55 {
            return simple_error!("Kernel image does not have the correct magic number");
        }

        if kernel_header.header != 0x53726448 {    // header should be "HdrS"
            return simple_error!("Kernel does not have a valid header");
        }

        if kernel_header.version < 0x020c {
            return simple_error!("Kernel does not support boot protocol version 2.12");
        }

        if kernel_header.relocatable_kernel == 0 {
            // kernels with protocol version >= 2.12 should be relocatable anyway
            return simple_error!("Kernel is not relocatable! This code only works for relocatable kernels :(");
        }
        Ok(())
    }

    fn extract_protected_mode_kernel_to_aligned_address(&mut self, boot_params: &mut BootParams) -> usize {
        let setup_code_size = boot_params.kernel_header.setup_sects as usize * 512;
        let protected_mode_kernel_start = (setup_code_size + 512).try_into().unwrap();

        let addr = crate::mem::copy_buf_to_aligned_address(&self.image[protected_mode_kernel_start..]);

        // "if a bootloader which does not install a hook loads a relocatable kernel at a nonstandard address it will have to modify this field to point to the load address."
        // https://www.kernel.org/doc/Documentation/x86/boot.txt
        boot_params.kernel_header.code32_start = addr as u32;

        println!("protected-mode kernel code copied to {:x}", addr);

        addr
    }

    fn set_cmdline(boot_params: &mut BootParams, cmdline: &str) -> SimpleResult<()> {
        let addr = allocate_low_pages(1)?;
        unsafe { core::ptr::write(addr as *mut &[u8], cmdline.as_bytes()); }
        boot_params.kernel_header.cmd_line_ptr = addr as u32;
        Ok(())
    }

    // TODO: check if ramdisk works correctly
    fn set_ramdisk(boot_params: &mut BootParams, ramdisk: Option<Vec<u8>>) {
        if let Some(ramdisk) = ramdisk {
            let ramdisk_addr = copy_buf_to_aligned_address(ramdisk.as_slice());

            boot_params.kernel_header.ramdisk_image = ramdisk_addr as u32;
            boot_params.kernel_header.ramdisk_size = ramdisk.len() as u32;
        } else {
            boot_params.kernel_header.ramdisk_image = 0;
            boot_params.kernel_header.ramdisk_size = 0;
        }
    }

    fn set_memory_map(boot_params: &mut BootParams, mmap: &MemoryMapOwned) {
        const MAX_E820_ENTRIES: usize = 128;

        let mut i = 0;
        for entry in mmap.entries() {
            let typ = match entry.ty {
                MemoryType::CONVENTIONAL => E820_TYPE_RAM,
                MemoryType::BOOT_SERVICES_CODE => E820_TYPE_RAM,
                MemoryType::BOOT_SERVICES_DATA => E820_TYPE_RAM,
                MemoryType::LOADER_CODE => E820_TYPE_RAM,
                MemoryType::LOADER_DATA => E820_TYPE_RAM,
                _ => E820_TYPE_RESERVED,
            };

            boot_params.e820_table[i] = E820Entry {
                addr: entry.phys_start,
                size: entry.page_count * 4096,
                typ,
            };

            i += 1;
            if i >= MAX_E820_ENTRIES {
                break;
            }
        }

        boot_params.e820_entries = i as u8;
    }

    // this is partially guessed and partially taken from grub2 source code because there was not much documentation available
    fn set_video_params(boot_params: &mut BootParams) {
        const VIDEO_TYPE_EFI: u8 = 0x70;    // linux kernel screen_info.h

        let screen_info = &mut boot_params.screen_info;

        let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
        let mut gop = open_protocol_unsafe::<GraphicsOutput>(gop_handle).unwrap();

        uefi::println!("GOP pixel format: {:?}", gop.current_mode_info().pixel_format());

        // pretty sure about these
        screen_info.orig_video_page = 0;
        screen_info.orig_video_points = 16;
        screen_info.lfb_width = gop.current_mode_info().resolution().0 as u16;
        screen_info.lfb_height = gop.current_mode_info().resolution().1 as u16;
        screen_info.lfb_depth = 32;
        screen_info.lfb_linelength = gop.current_mode_info().stride() as u16 * (screen_info.lfb_depth >> 3);
        screen_info.lfb_base = gop.frame_buffer().as_mut_ptr() as u32;
        screen_info.ext_lfb_base = (gop.frame_buffer().as_mut_ptr() as u64 >> 32) as u32;
        screen_info.capabilities |= 0b10;     // mark framebuffer address as 64 bit
        screen_info.lfb_size = gop.frame_buffer().size() as u32;
        
        screen_info.orig_video_ega_bx = 0;

        // for BGR8 (from grub2 source code)
        // TODO: support pixel formats other than BGR8
        screen_info.red_size = 8;
        screen_info.red_pos = 16;
        screen_info.green_size = 8;
        screen_info.green_pos = 8;
        screen_info.blue_size = 8;
        screen_info.blue_pos = 0;
        screen_info.rsvd_size = 8;
        screen_info.rsvd_pos = 24;

        // not so sure about these
        screen_info.orig_x = 0;
        screen_info.orig_y = 0;
        screen_info.orig_video_cols = 80;
        screen_info.orig_video_lines = 25;
        screen_info.orig_video_is_vga = VIDEO_TYPE_EFI;
        screen_info.orig_video_mode = 0;      // could also be 0x03

        // sketchy
        screen_info.ext_mem_k = ((32 * 0x100000) >> 10) as u16;

    }

    fn normal_handover(&mut self, mut boot_params: BootParams) -> SimpleResult<()> {
        println!("Starting using normal handover");

        let _low_pages_for_kernel = crate::mem::allocate_low_pages(10)?; // TODO: check if necessary

        let protected_mode_kernel_addr = self.extract_protected_mode_kernel_to_aligned_address(&mut boot_params);
        let entry_point = protected_mode_kernel_addr + 0x200; // 64bit entry point is at +0x200 of protected-mode code
        println!("Entry point is at {:x}", entry_point);

        Kernel::set_video_params(&mut boot_params);

        let gdtr = crate::mem::gdt::create_simple_gdtr();
        println!("Building page tables...");
        let pml4_ptr = unsafe { crate::mem::paging::prepare_identity_mapped_pml4() } as usize;

        println!("Exiting boot services, bye...");

        unsafe {
            let old_mmap = uefi::boot::exit_boot_services(MemoryType::LOADER_DATA);

            Kernel::set_memory_map(&mut boot_params, &old_mmap);
            crate::mem::gdt::set_gdtr(&gdtr);

            Kernel::run(pml4_ptr, entry_point, boot_params);
        }
    }

    fn efi_handover(&mut self, mut boot_params: BootParams) -> SimpleResult<()> {
        println!("Starting using efi handover");

        let protected_mode_kernel_addr = self.extract_protected_mode_kernel_to_aligned_address(&mut boot_params);

        // calculating entry point
        let entry_point_efi_64bit = protected_mode_kernel_addr + 0x200 + boot_params.kernel_header.handover_offset as usize;

        println!("Entering kernel, bye...");

        Kernel::jump_to_efi_entry(
            entry_point_efi_64bit,
            uefi::boot::image_handle().as_ptr() as usize,
            uefi::table::system_table_raw().unwrap().as_ptr() as usize,
            boot_params,
        );
    }

    pub fn start(&mut self, cmdline: &str, ramdisk: Option<Vec<u8>>) -> SimpleResult<()> {
        // copy kernel header into zero page (boot params)
        let mut boot_params = BootParams::new()?;
        boot_params.kernel_header = *KernelHeader::new(&self.image)?;

        // setting parameters shared by both handover methods
        Kernel::set_cmdline(&mut boot_params, cmdline)?;
        Kernel::set_ramdisk(&mut boot_params, ramdisk);

        boot_params.kernel_header.type_of_loader = 0xFF; // custom bootloader
        boot_params.kernel_header.vid_mode = 0xFFFF; // TODO: is this correct?

        if boot_params.kernel_header.xloadflags & 0b1000 == 0 {
            self.normal_handover(boot_params)
        } else {
            self.efi_handover(boot_params)
        }
    }

    fn jump_to_efi_entry(entry_point: usize, handle: usize, system_table: usize, boot_params: BootParams) -> ! {
        unsafe {
            asm!(
            r#"
            jmp {}
            "#,
            in(reg) entry_point,
            in("rdi") handle,
            in("rsi") system_table,
            in("rdx") (&boot_params) as *const BootParams as usize,
            );
        }
        unreachable!();
    }

    // Stolen from https://github.com/rust-osdev/bootloader/blob/main/common/src/lib.rs
    unsafe fn run(page_table_addr: usize, entry_point: usize, boot_params: BootParams) -> ! {
        unsafe {
            asm!(
                r#"
                xor rbp, rbp
                mov cr3, {}
                push 0
                jmp {}
                "#,
                in(reg) page_table_addr,
                in(reg) entry_point,
                in("rsi") (&boot_params) as *const BootParams as usize,
            );
        }
        unreachable!();
    }
}
