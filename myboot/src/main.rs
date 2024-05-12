#![no_main]
#![no_std]

use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::fmt::Write;

mod my_list_logic;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();

    const LIST: [&str; 5] = ["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

    my_list_logic::iterate_list(&mut st, &LIST);

    st.boot_services().stall(3_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
