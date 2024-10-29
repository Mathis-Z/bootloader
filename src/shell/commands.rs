extern crate alloc;
use core::ffi::CStr;

use alloc::{
    boxed::Box,
    string::{self, String, ToString},
    vec::Vec,
};
use uefi::{
    data_types::EqStrUntilNul,
    fs::{FileSystem, Path},
    CStr16, CString16, Char16,
};

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
        shell.println("This is the Yannik & Mathis boot shell :)\nAvailable commands are:");
        for program in [Program::HELP, Program::EXIT, Program::LS, Program::CLEAR] {
            shell.print("- ");
            shell.println(&program.name());
        }
    }

    fn ls(&mut self, shell: &mut Shell) {
        if self.args.len() > 1 {
            shell.println("ls takes at most one parameter");
            return;
        }

        let param = &mut CString16::new();
        if let Some(arg) = self.args.first() {
            param.push_str(arg);
        }
        param.replace_char(Char16!('/'), Char16!('\\')); // translate UNIX-like forward slashes

        match &mut shell.fs {
            None => {
                if param.is_empty() {
                    shell.println("The following FAT volumes are available:");
                    for volume_name in disk_helpers::get_volume_names(shell.st.boot_services()) {
                        shell.print("- ");
                        shell.println(&volume_name);
                    }
                } else {
                    let path = Path::new(param);
                    let mut path_components = path.components();

                    if let Some(volume_name) = path_components.next() {
                        let mut result;

                        if let Some(mut fs) = disk_helpers::open_volume_by_name(
                            shell.st.boot_services(),
                            &volume_name,
                        ) {
                            let mut rest_path = CString16::new();

                            for component in path_components {
                                rest_path.push(Char16!('\\'));
                                rest_path.push_str(&component);
                            }

                            result = Command::ls_fs(&mut fs, &CString16::new(), &rest_path);
                        } else {
                            result = CString16::try_from("Error: Could not open volume with name ")
                                .unwrap();
                            result.push_str(&volume_name);
                        }
                        shell.println(&result);
                    } else {
                        shell.println("Error: Could not get volume name from the path you entered");
                    }
                }
            }
            Some(fs) => {
                let s = Command::ls_fs(fs, &shell.cwd, &param);
                shell.println(&s);
            }
        }
    }

    fn ls_fs(fs: &mut FileSystem, cwd: &CString16, path: &CString16) -> CString16 {
        let mut output = CString16::new();

        let mut full_path = CString16::new();
        full_path.push_str(cwd);
        full_path.push_str(path);

        output.push_str(&full_path);
        output.push_str(&CString16::try_from("\n").unwrap());

        match fs.read_dir(Path::new(&full_path)) {
            Ok(contents) => {
                for fileinfo_result in contents {
                    match fileinfo_result {
                        Ok(fileinfo) => {
                            output.push_str(&fileinfo.file_name());
                            if fileinfo.is_directory() {
                                output.push(Char16::try_from('/').unwrap())
                            }
                            output.push(Char16::try_from('\n').unwrap())
                        }
                        Err(err) => {
                            output.push_str(
                                &CString16::try_from("Could not get file info: ").unwrap(),
                            );
                            output
                                .push_str(&CString16::try_from(err.to_string().as_str()).unwrap());
                        }
                    }
                }
            }
            Err(err) => {
                output.push_str(&CString16::try_from("Could not read directory: ").unwrap());
                output.push_str(&CString16::try_from(err.to_string().as_str()).unwrap());
            }
        }

        output
    }

    fn clear(&mut self, shell: &mut Shell) {
        if let Err(err) = shell.st.stdout().clear() {
            shell.print("Could not clear shell because of error: ");
            shell.debug_println(&err);
        }
    }

    fn exit(&mut self, shell: &mut Shell) {
        shell.exit = true;
    }
}
