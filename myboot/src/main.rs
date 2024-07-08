#![no_main]
#![no_std]

use shell::Shell;
use uefi::prelude::*;

mod disk_helpers;
mod handle_helpers;
mod memory;
mod my_list_logic;
mod shell;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();

    {
        let mut shell = Shell::new(&mut st);
        shell.enter();
    }

    st.boot_services().stall(2_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
