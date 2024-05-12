#![no_main]
#![no_std]

use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::fmt::Write;


const LIST: [&str; 5] = ["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

fn print_list(st: &mut SystemTable<Boot>, selected_index: usize) {
    
    for (index, item) in LIST.iter().enumerate() {
        if index == selected_index {
            write!(st.stdout(), "-> {}\r\n", item).expect("Write failed");
        } else {
            write!(st.stdout(), "   {}\r\n", item).expect("Write failed");
        }
    }

    for n in 1..20 {
        write!(st.stdout(), "\n");
    }
}

pub fn handle_input(st: &mut SystemTable<Boot>) {
    let mut selected_index = 0;
    print_list(st, selected_index);

    let exit_flag = false;

    while !exit_flag {
        let key = st.stdin().read_key().expect("Expected input");
        let _ = match key {
            Some(k) => {
                match k {
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::UP) => {
                        selected_index = core::cmp::max(selected_index - 1, 0);
                        print_list(st, selected_index);
                    }
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::DOWN) => {
                        selected_index = core::cmp::min(selected_index + 1, LIST.len() - 1);
                        print_list(st, selected_index);
                    }
                    uefi::proto::console::text::Key::Printable(p) => {
                        write!(st.stdout(), "A printable key was entered: {:?}\r\n", p).expect("Write failed");
                    }
                    uefi::proto::console::text::Key::Special(s) => {
                        write!(st.stdout(), "A special key was entered: {:?}\r\n", s).expect("Write failed");
                    }
                };
            }
            None => {}
        };
    }
}