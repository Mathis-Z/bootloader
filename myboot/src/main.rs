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

    memory::print_memory_map(st.boot_services());

    {
        let mut shell = Shell::new(&mut st);
        shell.enter();
    }
    // let efis = disk_helpers::search_efis(st.boot_services());

    // let selected_index = my_list_logic::iterate_list(&mut st, &efis);

    // let efi = efis
    //     .get(selected_index)
    //     .expect("selected index should not be out of bounds!");
    // disk_helpers::start_efi2(&_image_handle, st.boot_services(), efi);

    // st.boot_services().stall(200_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
