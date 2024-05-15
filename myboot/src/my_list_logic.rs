use core::fmt::Write;
use uefi::prelude::*;
use uefi::println;
use uefi::proto::console::text::Key;
use uefi::proto::console::text::ScanCode;
use uefi::table::boot::BootServices;
use uefi::{Char16, ResultExt};

fn print_list<T: AsRef<str>>(st: &mut SystemTable<Boot>, list: &[T], mut selected_index: usize) {
    // resets the screen
    st.stdout().reset(false);

    // goes through list and points at xth element
    for (index, item) in list.iter().enumerate() {
        if index == selected_index {
            write!(st.stdout(), "-> {}\r\n", item.as_ref()).expect("Write failed");
        } else {
            write!(st.stdout(), "   {}\r\n", item.as_ref()).expect("Write failed");
        }
    }
}

pub fn iterate_list<T: AsRef<str>>(st: &mut SystemTable<Boot>, list: &[T]) -> u16 {
    let mut selected_index: u16 = 0;
    let exit_key = Char16::try_from('\n').unwrap();

    print_list(st, list, selected_index);

    let exit_flag = false;

    while !exit_flag {
        let key = st.stdin().read_key().expect("Expected input");
        let _ = match key {
            Some(k) => {
                match k {
                    // checks if pressed key is ArrowUp
                    Key::Special(ScanCode::UP) => {
                        selected_index = selected_index.saturating_sub(1); // preventing segmentation fault
                        print_list(st, list, selected_index);
                    }
                    // checks if pressed key is ArrowDown
                    Key::Special(ScanCode::DOWN) => {
                        selected_index = selected_index.saturating_add(1).min(list.len() - 1); // preventing segmentation fault
                        print_list(st, list, selected_index);
                    }
                    Key::Printable(exit_key) => {
                        exit_flag = true;
                    }
                };
            }
            None => {}
        };
    }
    selected_index
}
