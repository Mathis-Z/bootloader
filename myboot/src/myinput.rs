use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::fmt::Write;


const LIST: [&str; 5] = ["Item 1", "Item 2", "Item 3", "Item 4", "Item 5"];

fn print_list(st: &mut SystemTable<Boot>, mut selected_index: usize) {
    // resets the screen
    st.stdout().reset(false);

    // goes through list and points at xth element
    for (index, item) in LIST.iter().enumerate() {
        if index == selected_index{
            write!(st.stdout(), "-> {}\r\n", item).expect("Write failed");
        } else {
            write!(st.stdout(), "   {}\r\n", item).expect("Write failed");
        }
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
                    // checks if pressed key is ArrowUp
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::UP) => {
                        selected_index = selected_index.saturating_sub(1); // preventing segmentation fault
                        print_list(st, selected_index);
                    }
                    // checks if pressed key is ArrowDown
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::DOWN) => {
                        selected_index = selected_index.saturating_add(1).min(LIST.len() - 1); // preventing segmentation fault
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