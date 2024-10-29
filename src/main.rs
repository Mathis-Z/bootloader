#![no_main]
#![no_std]

use kernel_loading::kernel_test;
use shell::*;
use uefi::prelude::*;

mod disk_helpers;
mod gdt;
mod handle_helpers;
mod kernel_loading;
mod kernel_params;
mod memory;
mod my_list_logic;
mod paging;
mod shell;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();

    // {
    //     let mut shell = Shell::new(&mut st);
    //     shell.enter();
    // }

    kernel_test(st);

    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
