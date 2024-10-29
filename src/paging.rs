use uefi::prelude::BootServices;

use crate::memory::*;

// this is the PML4 followed by the PDPT which are in turn followed by their PDs which are in turn followed by their PTs

const PRESENT: u64 = 1 << 0;
const RW: u64 = 1 << 1;
const USER_PAGE: u64 = 1 << 2;
const WRITE_THROUGH: u64 = 1 << 3;
const CACHE_DISABLE: u64 = 1 << 4;
const ACCESSED: u64 = 1 << 5;
const DIRTY: u64 = 1 << 6;
const LARGE_PAGE: u64 = 1 << 7;
const GLOBAL: u64 = 1 << 8;
const EXECUTE_DISABLE: u64 = 1 << 63;

const PT_FLAGS: u64 = PRESENT | RW;
const PD_FLAGS: u64 = PRESENT | RW;
const PDPT_FLAGS: u64 = PRESENT | RW;
const PML4_FLAGS: u64 = PRESENT | RW;

#[repr(C, packed)]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    fn allocate(bs: &BootServices) -> *mut PageTable {
        allocate_pages(bs, 1) as *mut PageTable
    }

    fn set_entry(&mut self, index: usize, address: u64, flags: u64) {
        self.entries[index] = address | flags;
    }
}

pub unsafe fn prepare_identity_mapped_pml4(bs: &BootServices) -> *mut PageTable {
    let pml4_ptr = PageTable::allocate(bs);

    (*pml4_ptr).set_entry(0, prepare_identity_mapped_pdpt(bs, 0) as u64, PML4_FLAGS);

    pml4_ptr
}

pub unsafe fn prepare_identity_mapped_pdpt(bs: &BootServices, address: usize) -> *mut PageTable {
    let pdpt_ptr = PageTable::allocate(bs);

    for pd_idx in 0..100 {
        // page tables for 100 GB RAM should be enough
        (*pdpt_ptr).set_entry(
            pd_idx,
            prepare_identity_mapped_pd(bs, address + pd_idx * 512 * 512 * 4096) as u64,
            PDPT_FLAGS,
        );
    }
    pdpt_ptr
}

pub unsafe fn prepare_identity_mapped_pd(bs: &BootServices, address: usize) -> *mut PageTable {
    let pd_ptr = PageTable::allocate(bs);

    for pt_idx in 0..512 {
        (*pd_ptr).set_entry(
            pt_idx,
            prepare_identity_mapped_pt(bs, address + pt_idx * 512 * 4096) as u64,
            PD_FLAGS,
        );
    }
    pd_ptr
}

pub unsafe fn prepare_identity_mapped_pt(bs: &BootServices, address: usize) -> *mut PageTable {
    let pt_ptr = PageTable::allocate(bs);

    for page_idx in 0..512 {
        (*pt_ptr).set_entry(page_idx, (address + page_idx * 4096) as u64, PT_FLAGS);
    }
    pt_ptr
}
