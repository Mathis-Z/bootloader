// These are some helper functions related to memory management.

extern crate alloc;

use uefi::boot::AllocateType;
use uefi::mem::memory_map::{MemoryMap, MemoryMapMut, MemoryType};
use uefi::println;

use crate::simple_error::{simple_error, SimpleResult};

pub(crate) mod gdt;
pub(crate) mod paging;

pub fn print_memory_map() {
    match uefi::boot::memory_map(MemoryType::LOADER_DATA) {
        Ok(mut memory_map) => {
            memory_map.sort();
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

pub fn allocate_pages(count: usize) -> usize {
    match uefi::boot::allocate_pages(
        AllocateType::MaxAddress(1 << 32), // stay under 4G to be safe (>4G could be fine with 64 bit but who knows)
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(mut dst) => {
            unsafe {
                core::ptr::write_bytes(dst.as_mut(), 0, 4096 * count); // zero out pages
            }
            dst.as_ptr() as usize
        }
        Err(err) => {
            println!("Error: failed to allocate pages due to error: {}", err);
            0
        }
    }
}

pub fn allocate_low_pages(count: usize) -> SimpleResult<usize> {
    match uefi::boot::allocate_pages(
        AllocateType::MaxAddress(0x100000),
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(mut dst) => {
            unsafe {
                core::ptr::write_bytes(dst.as_mut(), 0, 4096 * count); // zero out pages
            }
            Ok(dst.as_ptr() as usize)
        }
        Err(err) => {
            simple_error!("Error: failed to allocate pages due to error: {}", err)
        }
    }
}

pub fn copy_buf_to_aligned_address(buf: &[u8]) -> usize {
    const ALIGNMENT: usize = 1 << 21;

    let blocks_needed = (buf.len() - 1) / ALIGNMENT + 2 + 300;

    let page_count = blocks_needed * (ALIGNMENT / 4096);

    let dst = allocate_pages(page_count);
    let rounded_dst = (dst + ALIGNMENT - 1) & !(ALIGNMENT - 1);
    
    unsafe {
        core::ptr::copy(buf.as_ptr(), rounded_dst as *mut u8, buf.len());
    }

    rounded_dst
}
