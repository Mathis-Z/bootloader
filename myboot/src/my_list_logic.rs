use core::fmt::Display;
use core::fmt::Write;
use uefi::prelude::*;

use uefi::proto::console::text::Key;
use uefi::proto::console::text::ScanCode;
use uefi::Char16;

fn print_list<T: Display>(st: &mut SystemTable<Boot>, list: &[T], selected_index: usize) {
    // resets the screen
    let _ = st.stdout().reset(false);

    // goes through list and points at xth element
    for (index, item) in list.iter().enumerate() {
        if index == selected_index {
            write!(st.stdout(), "-> {}\r\n", item).expect("Write failed");
        } else {
            write!(st.stdout(), "   {}\r\n", item).expect("Write failed");
        }
    }
}

pub fn iterate_list<T: Display>(st: &mut SystemTable<Boot>, list: &[T]) -> usize {
    let mut selected_index: usize = 0;
    let exit_key = Char16::try_from('\r').unwrap(); // enter key is mapped to carriage return for some reason ?

    print_list(st, list, selected_index);

    let mut exit_flag = false;

    while !exit_flag {
        let key = st.stdin().read_key().expect("Expected input");
        let _ = match key {
            Some(k) => {
                match k {
                    // checks if pressed key is ArrowUp
                    Key::Special(ScanCode::UP) => {
                        selected_index = selected_index.saturating_sub(1); // preventing integer wrap-around
                        print_list(st, list, selected_index);
                    }
                    // checks if pressed key is ArrowDown
                    Key::Special(ScanCode::DOWN) => {
                        selected_index = selected_index.saturating_add(1).min(list.len() - 1); // preventing integer wrap-around
                        print_list(st, list, selected_index);
                    }
                    Key::Printable(key) => {
                        if key == exit_key {
                            exit_flag = true;
                        }
                    }
                    _ => {}
                };
            }
            None => {}
        };
    }
    let _ = st.stdout().reset(false);

    selected_index
}
