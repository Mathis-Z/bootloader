#![no_main]
#![no_std]

use memory::print_memory_map;
use shell::*;
use uefi::prelude::*;

mod disk_helpers;
mod kernel_loading;
mod kernel_params;
mod memory;
mod shell;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();

    let mut shell = Shell::new(st, _image_handle);
    shell.enter();

    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
