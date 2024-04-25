#![no_main]
#![no_std]


use uefi::prelude::*;
use uefi::println;
use log::info;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::prelude::v1::Ok;
use core::fmt::Write;



#[entry]
fn main(_image_handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();
    
    println!("Hello World!");

    let mut exit_flag = false;

    while !exit_flag {
        let key = st.stdin().read_key().expect("Expected input");
        let _ = match key {
            Some(k) =>  {
                match k {
                    uefi::proto::console::text::Key::Printable(p) => {
                        write!(st.stdout(), "A printable key was entered: {:?}\r\n", p).expect("Write failed");
                        if p == Char16::try_from(27u16).expect("Unable to convert the ESC ascii code to Char16") {
                            exit_flag = true;
                        }
                    }
                    uefi::proto::console::text::Key::Special(s) => {
                        write!(st.stdout(), "A special key was entered: {:?}\r\n", s).expect("Write failed");
                        if s == ScanCode::ESCAPE {
                            exit_flag = true;
                        }
                    }
                };             
            },
            None => {}
        };
    }
    
    st.boot_services().stall(3_000_000);
    Status::SUCCESS
}


#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
