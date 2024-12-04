#![no_main]
#![no_std]

mod disk;
mod kernel;
mod mem;
mod shell;
mod simple_error;

use shell::*;
use uefi::{prelude::*, println};

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    let _ = system::with_stdout(|stdout| stdout.clear());

    Shell::new().enter();

    Status::SUCCESS
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("================= PANIC =================");
    println!("{info}");
    loop {}
}
