extern crate alloc;

use core::slice;

use alloc::vec::Vec;
use uefi::{prelude::*, println, table::boot::AllocateType};
use uefi_raw::{table::boot::MemoryType, PhysicalAddress};

pub fn print_memory_map(bs: &BootServices) {
    let memory_map_size = bs.memory_map_size();

    let mut buf: [u8; 20000] = [0; 20000];

    match bs.memory_map(&mut buf) {
        Ok(mut memory_map) => {
            memory_map.sort();
            let mut i = 0;
            for md in memory_map.entries() {
                println!(
                    "phys {:#X} virt {:#X} size {} ty {:?}",
                    md.phys_start,
                    md.virt_start,
                    md.page_count * 4096,
                    md.ty
                );
            }
        }
        Err(err) => println!("Could not get memory map because of: {}", err),
    }
}

pub fn allocate_pages(bs: &BootServices, count: usize) -> u64 {
    match bs.allocate_pages(
        AllocateType::MaxAddress(1 << 32), // stay under 4G to be safe (>4G could be fine with 64 bit but who knows)
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(dst) => {
            unsafe {
                core::ptr::write_bytes(dst as *mut u8, 0, 4096 * count); // zero out pages
            }
            dst
        }
        Err(err) => {
            println!("Error: failed to allocate pages due to error: {}", err);
            0
        }
    }
}

pub fn allocate_low_pages(bs: &BootServices, count: usize) -> u64 {
    match bs.allocate_pages(
        AllocateType::MaxAddress(0x100000),
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(dst) => {
            unsafe {
                core::ptr::write_bytes(dst as *mut u8, 0, 4096 * count); // zero out pages
            }
            dst
        }
        Err(err) => {
            println!("Error: failed to allocate pages due to error: {}", err);
            0
        }
    }
}

pub fn copy_buf_to_aligned_address(bs: &BootServices, buf: &[u8]) -> PhysicalAddress {
    let page_count = (buf.len() - 1) / 4096 + 1;

    let dst = allocate_pages(bs, page_count);
    unsafe {
        core::ptr::copy(buf.as_ptr(), dst as *mut u8, buf.len());
    }
    dst
}

pub fn copy_buf_to_low_aligned_address(bs: &BootServices, buf: &[u8]) -> PhysicalAddress {
    let page_count = (buf.len() - 1) / 4096 + 1;

    let dst = allocate_low_pages(bs, page_count);
    unsafe {
        core::ptr::copy(buf.as_ptr(), dst as *mut u8, buf.len());
    }
    dst
}
