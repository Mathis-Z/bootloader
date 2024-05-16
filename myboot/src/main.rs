#![no_main]
#![no_std]

use uefi::prelude::*;
use uefi::println;

mod disk_helpers;
mod handle_helpers;
mod my_list_logic;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();

    let efis = disk_helpers::search_efis(st.boot_services());

    let selected_index = my_list_logic::iterate_list(&mut st, &efis);

    let efi = efis
        .get(selected_index)
        .expect("selected index should not be out of bounds!");
    disk_helpers::start_efi(&_image_handle, st.boot_services(), efi);

    st.boot_services().stall(2_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
