#![no_main]
#![no_std]

mod disk;
mod kernel;
mod mem;
mod shell;
mod simple_error;

use shell::*;
use uefi::prelude::*;

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
