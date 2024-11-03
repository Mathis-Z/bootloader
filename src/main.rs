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

static mut global_system_table: Option<SystemTable<Boot>> = None;
static mut global_image_handle: Option<Handle> = None;

pub fn system_table() -> &'static mut SystemTable<Boot> {
    unsafe { global_system_table.as_mut().unwrap() }
}

pub fn boot_services() -> &'static BootServices {
    system_table().boot_services()
}

pub fn image_handle() -> &'static mut Handle {
    unsafe { global_image_handle.as_mut().unwrap() }
}

#[entry]
fn main(image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    let _ = st.stdout().clear();

    unsafe {
        global_system_table = Some(st);
        global_image_handle = Some(image_handle)
    }

    Shell::new().enter();

    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
