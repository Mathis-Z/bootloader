// use uefi::prelude::BootServices;

// use crate::memory::*;

// // this is the PML4 followed by the PDPT which are in turn followed by their PDs which are in turn followed by their PTs

// const PRESENT: u64 = 1 << 0;
// const RW: u64 = 1 << 1;
// const PAGE_TABLE_FLAGS: u64 = PRESENT | RW;

// #[repr(C)]
// struct PageTable {
//     entries: [u64; 512],
// }

// impl PageTable {
//     fn allocate(bs: &BootServices) -> *mut PageTable {
//         allocate_pages(bs, 1) as *mut PageTable
//     }

//     fn set_entry(&mut self, index: usize, address: u64, flags: u64) {
//         self.entries[index] = address | flags;
//     }
// }

// pub fn setup_identity_mapping(bs: &BootServices) -> *mut PageTable {
//     unsafe {
//         let pml4_ptr = PageTable::allocate(bs);
//         let mut pdpt_ptr = PageTable::allocate(bs);
//         let mut pd_ptr = PageTable::allocate(bs);

//         let pml4_table = pml4_ptr.as_mut().unwrap();
//         let pdpt_table = pdpt_ptr.as_mut().unwrap();

//         // Map the PDPT in the PML4
//         pml4_table.set_entry(0, pdpt_ptr as *const _ as u64, PAGE_TABLE_FLAGS);

//         // Map the PD in the PDPT
//         pdpt_table.set_entry(0, pd_ptr as *const _ as u64, PAGE_TABLE_FLAGS);

//         // Identity-map the first 1 GB of memory in the PD
//         for i in 0..512 {
//             pd_table.set_entry(i, (i as u64) << 21, PAGE_TABLE_FLAGS);
//         }

//         pml4_table
//     }
// }
