#![no_main]
#![no_std]

use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::fmt::Write;

mod myinput;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();

    myinput::handle_input(&mut st);

    st.boot_services().stall(3_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
