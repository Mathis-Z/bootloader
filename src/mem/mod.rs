extern crate alloc;

use uefi::boot::AllocateType;
use uefi::mem::memory_map::{MemoryMap, MemoryMapMut, MemoryType};
use uefi::println;
use uefi_raw::PhysicalAddress;

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

pub fn allocate_pages(count: usize) -> u64 {
    match uefi::boot::allocate_pages(
        AllocateType::MaxAddress(1 << 32), // stay under 4G to be safe (>4G could be fine with 64 bit but who knows)
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(mut dst) => {
            unsafe {
                core::ptr::write_bytes(dst.as_mut(), 0, 4096 * count); // zero out pages
            }
            dst.as_ptr() as u64
        }
        Err(err) => {
            println!("Error: failed to allocate pages due to error: {}", err);
            0
        }
    }
}

pub fn allocate_low_pages(count: usize) -> u64 {
    match uefi::boot::allocate_pages(
        AllocateType::MaxAddress(0x100000),
        MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(mut dst) => {
            unsafe {
                core::ptr::write_bytes(dst.as_mut(), 0, 4096 * count); // zero out pages
            }
            dst.as_ptr() as u64
        }
        Err(err) => {
            println!("Error: failed to allocate pages due to error: {}", err);
            0
        }
    }
}

pub fn copy_buf_to_aligned_address(buf: &[u8]) -> PhysicalAddress {
    let page_count = (buf.len() - 1) / 4096 + 1;

    let dst = allocate_pages(page_count);
    unsafe {
        core::ptr::copy(buf.as_ptr(), dst as *mut u8, buf.len());
    }
    dst
}
