extern crate alloc;
use alloc::{
    string::{self, String},
    vec::Vec,
};
use uefi::{data_types::EqStrUntilNul, CString16};

use crate::*;

pub enum Program {
    HELP,
    EXIT,
    LS,
    CLEAR,
}

pub struct Command {
    pub program: Program,
    pub args: Vec<CString16>,
}

impl Program {
    pub fn name(&self) -> String {
        String::from(match self {
            Program::HELP => "help",
            Program::EXIT => "exit",
            Program::LS => "ls",
            Program::CLEAR => "clear",
        })
    }

    pub fn from(string: CString16) -> Option<Program> {
        match string {
            _ if string.eq_str_until_nul("help") => return Some(Program::HELP),
            _ if string.eq_str_until_nul("exit") => return Some(Program::EXIT),
            _ if string.eq_str_until_nul("ls") => return Some(Program::LS),
            _ if string.eq_str_until_nul("clear") => return Some(Program::CLEAR),
            _ => return None,
        }
    }
}

impl Command {
    pub fn execute(&mut self, shell: &mut Shell) {
        match self.program {
            Program::HELP => self.help(shell),
            Program::EXIT => self.exit(shell),
            Program::LS => self.ls(shell),
            Program::CLEAR => self.clear(shell),
        }
    }

    fn help(&mut self, shell: &mut Shell) {
        shell.println(&"This is the Yannik & Mathis boot shell :)\nAvailable commands are:");
        for program in [Program::HELP, Program::EXIT, Program::LS, Program::CLEAR] {
            shell.print(&"- ");
            shell.println(&program.name());
        }
    }

    fn ls(&mut self, shell: &mut Shell) {
        shell.println(&CString16::try_from("Hier könnte Ihre Werbung stehen!").unwrap());
    }

    fn clear(&mut self, shell: &mut Shell) {
        if let Err(err) = shell.st.stdout().clear() {
            shell.print(&"Could not clear shell because of error: ");
            shell.debug_println(&err);
        }
    }

    fn exit(&mut self, shell: &mut Shell) {
        shell.exit = true;
    }
}
