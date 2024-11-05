#![no_main]
#![no_std]

use shell::*;
use uefi::prelude::*;

mod disk_helpers;
mod gdt;
mod kernel_loading;
mod kernel_params;
mod memory;
mod paging;
mod shell;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    let _ = system::with_stdout(|stdout| stdout.clear());

    Shell::new().enter();

    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
