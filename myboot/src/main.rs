#![no_main]
#![no_std]


use uefi::prelude::*;
use uefi::println;

#[entry]
fn main(_image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut system_table).unwrap();
    
    println!("Hello World!");
    
    system_table.boot_services().stall(3_000_000);
    Status::SUCCESS
}


#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
