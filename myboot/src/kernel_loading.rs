extern crate alloc;

use core::arch::asm;

use alloc::vec::Vec;
use uefi::{
    fs::{FileSystem, Path},
    prelude::*,
    println, CString16,
};

use crate::{
    kernel_params,
    memory::{self},
};

use kernel_params::*;

pub struct Kernel {
    blob: Vec<u8>,
    kernel_params: KernelParams,
}

impl Kernel {
    pub fn load_and_start(
        handle: &Handle,
        st: &SystemTable<Boot>,
        cmdline: &CString16,
        fs: &mut FileSystem,
        path: &CString16,
    ) {
        if let Some(mut kernel) = Kernel::load_from(fs, path) {
            if !kernel.check_magic_number() {
                println!("Kernel image does not start with MZ magic number!");
                return;
            }

            kernel.start(handle, st, cmdline);
        } else {
            println!("Kernel not loaded");
        }
    }

    pub fn check_magic_number(&self) -> bool {
        if self.blob[0] != 0x4d {
            println!("Blob does not start with M!");
            return false;
        }
        if self.blob[1] != 0x5a {
            println!("Blob does not start with MZ!");
            return false;
        }
        true
    }

    pub fn load_from(fs: &mut FileSystem, path: &CString16) -> Option<Kernel> {
        println!("Loading kernel...");

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

    pub fn start(&mut self, handle: &Handle, st: &SystemTable<Boot>, cmdline: &CString16) {
        let bs = st.boot_services();

        if self.kernel_params.get_param(KernelParam::RelocatableKernel) == 0 {
            println!("Kernel is not relocatable! This code only works for relocatable kernels :(");
            return;
        }

        if self.kernel_params.get_param(KernelParam::XLoadflags) & 0b100 == 0 {
            println!("Kernel image does not support 64 bit EFI handover. This code only works with EFI handover. Bye...");
            bs.stall(2_000_000);
        }

        // copying protected mode kernel to aligned address (actually the alignment should be 2M but this still works)
        let setup_code_size = self.kernel_params.get_param(KernelParam::SetupSects) * 512;
        let protected_mode_kernel_start: usize = (setup_code_size + 512).try_into().unwrap();
        let protected_mode_kernel_addr =
            memory::copy_buf_to_aligned_address(bs, &self.blob[protected_mode_kernel_start..]);

        // calculating entry point
        let handover_offset = self.kernel_params.get_param(KernelParam::HandoverOffset);
        let entry_point_efi_64bit = protected_mode_kernel_addr + 0x200 + handover_offset;

        st.boot_services().stall(1_000_000); // time to pause with debugger

        // copying cmdline string to page aligned address
        let cmdline_addr = allocate_cmdline(bs, cmdline);

        // setting required params
        self.kernel_params
            .set_param(KernelParam::Code32Start, protected_mode_kernel_addr); // not sure if correct but it works
        self.kernel_params
            .set_param(KernelParam::CmdLinePtr, cmdline_addr);
        self.kernel_params.set_param(KernelParam::RamdiskImage, 0); // no ramdisk
        self.kernel_params.set_param(KernelParam::RamdiskSize, 0);

        // zero page has space for other parameters (that we do not have to set with the EFI handover protocol)
        let zero_page_addr = self.kernel_params.copy_into_zero_page(bs);

        println!("Entering kernel, bye...");

        Kernel::jump_to_efi_entry(
            entry_point_efi_64bit,
            handle.as_ptr() as u64,
            st.as_ptr() as u64,
            zero_page_addr,
        );
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
}
