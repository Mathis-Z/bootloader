extern crate alloc;

use core::{arch::asm, mem::size_of};

use alloc::{boxed::Box, vec::Vec};
use uefi::{
    fs::{FileSystem, Path},
    prelude::*,
    print, println,
    table::boot::AllocateType,
    CString16, Char16,
};
use uefi_raw::{
    table::boot::{MemoryAttribute, MemoryDescriptor, MemoryType},
    PhysicalAddress,
};

use crate::{
    disk_helpers::open_volume_by_name,
    gdt::{allocate_page_for_gdt, create_and_set_simple_gdt},
    kernel_params,
    memory::{self, allocate_low_pages},
};

use kernel_params::*;

struct Kernel {
    blob: Vec<u8>,
    kernel_params: KernelParams,
}

impl Kernel {
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

    pub fn start(&mut self, handle: &Handle, st: SystemTable<Boot>) {
        let bs = st.boot_services();

        if self.kernel_params.get_param(KernelParam::RelocatableKernel) == 0 {
            println!("Kernel is not relocatable! This code only works for relocatable kernels :(");
            return;
        }

        let setup_code_size = self.kernel_params.get_param(KernelParam::SetupSects) * 512;
        let protected_mode_kernel_start: usize = (setup_code_size + 512).try_into().unwrap();
        let protected_mode_kernel_addr =
            memory::copy_buf_to_aligned_address(bs, &self.blob[protected_mode_kernel_start..]);
        println!(
            "protected-mode kernel code copied to {:x}",
            protected_mode_kernel_addr
        );

        if self.kernel_params.get_param(KernelParam::XLoadflags) & 0b100 == 0 {
            println!("Kernel image does not support 64 bit EFI handover. This code only works with EFI handover. Bye...");
            st.boot_services().stall(2_000_000);
        }
        let handover_offset = self.kernel_params.get_param(KernelParam::HandoverOffset);
        let entry_point_efi_64bit = protected_mode_kernel_addr + 0x200 + handover_offset;
        println!("Entry point is at {:x}", entry_point_efi_64bit);

        st.boot_services().stall(1_000_000); // time to pause with debugger

        let cmdline_addr = allocate_cmdline(bs);
        println!("Cmdline allocated at {:x}", cmdline_addr);

        // setting params (there are other params that hopefully are not required for 64bit...)

        self.kernel_params
            .set_param(KernelParam::Code32Start, protected_mode_kernel_addr); // might be wrong
        self.kernel_params
            .set_param(KernelParam::CmdLinePtr, cmdline_addr);
        self.kernel_params.set_param(KernelParam::RamdiskImage, 0);
        self.kernel_params.set_param(KernelParam::RamdiskSize, 0);

        let zero_page_addr = self.kernel_params.copy_into_zero_page(bs);

        println!("Entering kernel, bye...");

        Kernel::jump_to_efi_entry(
            entry_point_efi_64bit,
            handle.as_ptr() as u64,
            st.as_ptr() as u64,
            zero_page_addr,
        );
    }

    /// https://github.com/rust-osdev/bootloader/blob/main/common/src/lib.rs
    // unsafe fn _run(page_table_addr: u64, stack_top: u64, entry_point: u64, boot_info: u64) -> ! {
    //     unsafe {
    //         asm!(
    //             r#"
    //             xor rbp, rbp
    //             mov cr3, {}
    //             mov rsp, {}
    //             push 0
    //             jmp {}
    //             "#,
    //             in(reg) page_table_addr,
    //             in(reg) stack_top,
    //             in(reg) entry_point,
    //             in("rdi") boot_info as *const u64 as usize,
    //         );
    //     }
    //     unreachable!();
    // }

    unsafe fn run(stack_top: u64, entry_point: u64, boot_info: u64) -> ! {
        unsafe {
            asm!(
                r#"
                xor rbp, rbp
                mov rsp, {}
                push 0
                jmp {}
                "#,
                in(reg) stack_top,
                in(reg) entry_point,
                in("rsi") boot_info as *const u64 as usize,
            );
        }
        unreachable!();
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

pub fn kernel_test(handle: &Handle, st: SystemTable<Boot>) {
    let mut kernel = None;

    if let Some(mut fs) = open_volume_by_name(
        st.boot_services(),
        &CString16::try_from("0xbe939b98").unwrap(),
    ) {
        kernel = Kernel::load_from(
            &mut fs,
            &CString16::try_from("\\EFI\\ubuntu\\vmlinuz").unwrap(),
        )
    }

    if let Some(mut kernel) = kernel {
        kernel.start(handle, st);
    }
}
