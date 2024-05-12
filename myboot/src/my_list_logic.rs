use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};
use core::fmt::Write;


fn print_list<T: AsRef<str>>(st: &mut SystemTable<Boot>, list: &[T], mut selected_index: usize) {
    // resets the screen
    st.stdout().reset(false);

    // goes through list and points at xth element
    for (index, item) in list.iter().enumerate() {
        if index == selected_index{
            write!(st.stdout(), "-> {}\r\n", item.as_ref()).expect("Write failed");
        } else {
            write!(st.stdout(), "   {}\r\n", item.as_ref()).expect("Write failed");
        }
    }
}

pub fn iterate_list<T: AsRef<str>>(st: &mut SystemTable<Boot>, list: &[T]) {
    let mut selected_index = 0;

    print_list(st, list, selected_index);

    let exit_flag = false;

    while !exit_flag {
        let key = st.stdin().read_key().expect("Expected input");
        let _ = match key {
            Some(k) => {
                match k {
                    // checks if pressed key is ArrowUp
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::UP) => {
                        selected_index = selected_index.saturating_sub(1); // preventing segmentation fault
                        print_list(st, list, selected_index);
                    }
                    // checks if pressed key is ArrowDown
                    uefi::proto::console::text::Key::Special(uefi::proto::console::text::ScanCode::DOWN) => {
                        selected_index = selected_index.saturating_add(1).min(list.len() - 1); // preventing segmentation fault
                        print_list(st, list, selected_index);
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