extern crate alloc;

use alloc::{
    boxed::Box,
    fmt,
    string::{String, ToString},
    vec::{self, Vec},
};

use uefi::{
    fs::{FileSystem, Path},
    println,
    proto::console::text::{Key, ScanCode},
    table::{Boot, SystemTable},
    CString16, Char16, Error, Handle, StatusExt,
};
use uefi_raw::Status;

use core::fmt::Display;
use core::fmt::Write;
use core::{ffi::CStr, fmt::Debug};

use self::commands::{Command, Program};

mod commands;

#[macro_export]
macro_rules! CString16 {
    ($a:expr) => {{
        CString16::try_from($a).unwrap()
    }};
}

#[macro_export]
macro_rules! Char16 {
    ($a:expr) => {{
        Char16::try_from($a).unwrap()
    }};
}

pub struct Shell<'s> {
    cmd_history_idx: usize,
    cmd_history: Vec<CString16>,
    fs: Option<FileSystem<'s>>,
    cwd: CString16,
    st: &'s mut SystemTable<Boot>,
    pub exit: bool,
}

impl<'s> Shell<'s> {
    pub fn new(st: &mut SystemTable<Boot>, _image_handle: Handle) -> Shell {
        Shell {
            fs: None,
            cwd: CString16::new(),
            cmd_history_idx: 0,
            cmd_history: Vec::new(),
            exit: false,
            st,
        }
    }

    pub fn enter(&mut self) {
        Command {
            program: Program::HELP,
            args: Vec::new(),
        }
        .execute(self);

        self.st.stdout().enable_cursor(true);

        while !self.exit {
            self.print_shell();
            let line = self.read_line();
            self.execute_command_string(line);
        }
    }

    pub fn read_line(&mut self) -> CString16 {
        let mut line = Vec::<Char16>::new();

        self.cmd_history_idx = self.cmd_history.len();

        loop {
            let key = self.st.stdin().read_key().expect("Expected input");
            match key {
                Some(k) => {
                    match k {
                        Key::Special(ScanCode::UP) => {
                            self.cmd_history_idx = self.cmd_history_idx.saturating_sub(1);

                            if self.cmd_history_idx < self.cmd_history.len() {
                                self.clear_shell_line(line.len());
                                line = Vec::new();

                                for char in self.cmd_history[self.cmd_history_idx].iter() {
                                    line.push(*char);
                                }

                                for char in &line {
                                    self.print(&char);
                                }
                            }
                        }
                        // checks if pressed key is ArrowDown
                        Key::Special(ScanCode::DOWN) => {
                            if self.cmd_history_idx < self.cmd_history.len() - 1 {
                                self.cmd_history_idx = self.cmd_history_idx.saturating_add(1);

                                self.clear_shell_line(line.len());
                                line = Vec::new();

                                for char in self.cmd_history[self.cmd_history_idx].iter() {
                                    line.push(*char);
                                }

                                for char in &line {
                                    self.print(&char);
                                }
                            }
                        }

                        Key::Printable(key) => {
                            if key == '\r' {
                                self.print("\r\n");
                                let mut s = CString16::new();
                                for char in line {
                                    s.push(char)
                                }
                                return s;
                            } else if key == '\x08' {
                                if line.pop() != None {
                                    self.print(&key);
                                }
                            } else {
                                self.print(&key);
                                line.push(key);
                            }
                        }
                        _ => {}
                    };
                }
                None => {}
            }
        }
    }

    pub fn newline(&mut self) {
        write!(self.st.stdout(), "\n").expect("Write failed");
    }

    pub fn print<T: Display + ?Sized>(&mut self, text: &T) {
        write!(self.st.stdout(), "{}", text).expect("Write failed");
    }

    pub fn println<T: Display + ?Sized>(&mut self, text: &T) {
        write!(self.st.stdout(), "{}\n", text).expect("Write failed");
    }

    pub fn debug_print<T: Debug>(&mut self, text: &T) {
        write!(self.st.stdout(), "{:?}", text).expect("Write failed");
    }

    pub fn debug_println<T: Debug>(&mut self, text: &T) {
        write!(self.st.stdout(), "{:?}\n", text).expect("Write failed");
    }

    pub fn print_shell(&mut self) {
        self.print(&CString16::try_from(">> ").unwrap())
    }

    pub fn clear_shell_line(&mut self, chars_to_clear: usize) {
        for i in 0..chars_to_clear {
            self.print(&Char16::try_from('\x08').unwrap());
        }
    }

    pub fn execute_command_string(&mut self, command: CString16) {
        if let Some(mut parsed_cmd) = self.parse_command(&command) {
            parsed_cmd.execute(self);
        }
        if !command.is_empty() {
            self.cmd_history.push(command);
        }
    }

    pub fn parse_command(&self, command: &CString16) -> Option<Command> {
        let mut cmd_parts = Vec::<CString16>::new();
        let mut new_cmd_part = CString16::new();
        let mut escaped = false;
        let mut single_quoted = false;
        let mut double_quoted = false;

        for character in command.iter() {
            if *character == Char16!('\\') {
                if escaped {
                    new_cmd_part.push(*character);
                }
                escaped = !escaped;
            } else if *character == Char16!(' ') {
                if escaped {
                    return None;
                } else {
                    if single_quoted || double_quoted {
                        new_cmd_part.push(*character);
                    } else if !new_cmd_part.is_empty() {
                        cmd_parts.push(new_cmd_part);
                        new_cmd_part = CString16::new();
                    }
                }
            } else if *character == Char16!('\'') {
                if escaped || double_quoted {
                    new_cmd_part.push(*character);
                } else {
                    single_quoted = !single_quoted;
                }
            } else if *character == Char16!('\"') {
                if escaped || single_quoted {
                    new_cmd_part.push(*character);
                } else {
                    double_quoted = !double_quoted;
                }
            } else {
                new_cmd_part.push(*character)
            }
        }

        if !new_cmd_part.is_empty() {
            cmd_parts.push(new_cmd_part);
        }

        if cmd_parts.len() == 0 {
            None
        } else {
            let program = Program::from(cmd_parts.remove(0))?;

            Some(Command {
                program,
                args: cmd_parts,
            })
        }
    }
}
