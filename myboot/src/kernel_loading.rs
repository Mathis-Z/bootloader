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
    memory::*,
    paging::*,
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

    pub fn start(&mut self, st: SystemTable<Boot>) {
        let bs = st.boot_services();

        if self.kernel_params.get_param(KernelParam::RelocatableKernel) == 0 {
            println!("Kernel is not relocatable! This code only works for relocatable kernels :(");
            return;
        }

        let low_pages_for_kernel = allocate_low_pages(bs, 1000);

        let setup_code_size = self.kernel_params.get_param(KernelParam::SetupSects) * 512;
        let protected_mode_kernel_start: usize = (setup_code_size + 512).try_into().unwrap();

        let cmdline_addr = allocate_cmdline(bs);
        let stack_top_addr = allocate_low_pages(bs, 8) + 8 * 4096;
        let heap_end_addr = allocate_low_pages(bs, 8) + 8 * 4096;

        println!(
            "Stack top is at {:x}, Heap end is at {:x}",
            stack_top_addr, heap_end_addr
        );

        println!("Cmdline allocated at {:x}", cmdline_addr);

        // copying protected mode code to aligned address
        let protected_mode_slice = &self.blob[protected_mode_kernel_start..];
        let protected_mode_kernel_addr =
            allocate_pages_aligned_to_2M(bs, protected_mode_slice.len() / 4096);
        unsafe {
            core::ptr::copy(
                protected_mode_slice.as_ptr(),
                protected_mode_kernel_addr as *mut u8,
                protected_mode_slice.len(),
            );
        }

        println!(
            "protected-mode kernel code copied to {:x}",
            protected_mode_kernel_addr
        );

        let entry_point = protected_mode_kernel_addr + 0x200; // 64bit entry point is at +0x200 of protected-mode code

        println!("Entry point is at {:x}", entry_point);

        println!(
            "Kernel requires min alignment: {:x}",
            self.kernel_params.get_param(KernelParam::MinAlignment)
        );

        println!(
            "Kernel wants alignment: {:x}",
            self.kernel_params.get_param(KernelParam::KernelAlignment)
        );

        // setting params (there are other params that hopefully are not required for 64bit...)

        self.kernel_params
            .set_param(KernelParam::TypeOfLoader, 0xFF);
        self.kernel_params
            .set_param(KernelParam::CmdLinePtr, cmdline_addr);
        self.kernel_params
            .set_param(KernelParam::HeapEndPtr, heap_end_addr);
        self.kernel_params.set_param(KernelParam::VidMode, 0xFFFF);

        if self.kernel_params.get_param(KernelParam::CmdLinePtr) != cmdline_addr {
            println!("Failed to set cmd_line_ptr kernel param!");
        }

        println!("kernel params set");

        let zero_page_addr = self.kernel_params.copy_into_zero_page(bs);

        println!(
            "kernel params copied into zero page at {:x}",
            zero_page_addr
        );

        let gdt_page = allocate_page_for_gdt(bs);
        println!("Building page tables...");
        let pml4_ptr = unsafe { prepare_identity_mapped_pml4(bs) };
        println!("Page tables built at: {:x}", pml4_ptr as u64);

        let mut buf: [u8; 20000] = [0; 20000];
        let mmap_size = bs.memory_map_size();
        let entries = mmap_size.map_size / mmap_size.entry_size;
        println!("MMap has {entries} entries.");
        let Ok(mmap) = bs.memory_map(&mut buf) else {
            println!("Could not fetch mmap!");
            return;
        };

        KernelParams::set_memory_map(zero_page_addr, &mmap, low_pages_for_kernel);

        println!("Exiting boot services. Goodbye");
        st.boot_services().stall(2_000_000);

        unsafe {
            let (runtime_st, old_mmap) = st.exit_boot_services(MemoryType::LOADER_DATA);

            KernelParams::set_memory_map(zero_page_addr, &old_mmap, low_pages_for_kernel);

            // let st_address = runtime_st.get_current_system_table_addr();
            // let mut total_page_num = 0;
            // for memory_descriptor in old_mmap.entries() {
            //     total_page_num += memory_descriptor.page_count;
            // }

            // let mut identity_mmap = [MemoryDescriptor {
            //     ty: MemoryType::CONVENTIONAL,
            //     phys_start: 0,
            //     virt_start: 0,
            //     page_count: total_page_num,
            //     att: MemoryAttribute::all(),
            // }];

            // runtime_st.set_virtual_address_map(&mut identity_mmap, st_address);
            create_and_set_simple_gdt(gdt_page);

            Kernel::run(pml4_ptr as u64, stack_top_addr, entry_point, zero_page_addr);
        }
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

    // unsafe fn run(stack_top: u64, entry_point: u64, boot_info: u64) -> ! {
    //     unsafe {
    //         asm!(
    //             r#"
    //             xor rbp, rbp
    //             mov rsp, {}
    //             push 0
    //             jmp {}
    //             "#,
    //             in(reg) stack_top,
    //             in(reg) entry_point,
    //             in("rsi") boot_info as *const u64 as usize,
    //         );
    //     }
    //     unreachable!();
    // }
}

pub fn kernel_test(st: SystemTable<Boot>) {
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
        kernel.start(st);
    }
}
