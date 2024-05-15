#![no_main]
#![no_std]

use uefi::prelude::*;
use uefi::println;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::LoadImageSource;

mod disk_helpers;
mod handle_helpers;

#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();
    let bs = st.boot_services();

    println!("Hello World!");

    for name in disk_helpers::get_volume_names(bs) {
        println!("Volume: {}", name);
    }

    let efis = disk_helpers::search_efis(bs);

    for efi in &efis {
        let volume_name = disk_helpers::get_volume_name(bs, efi.file_system_handle);
        println!("EFI found at: {}\\{}", volume_name, efi.file_path);
    }

    st.boot_services().stall(100_000_000);
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
