extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use uefi::{
    data_types::EqStrUntilNul,
    fs::{FileSystem, Path, PathBuf},
    println, CString16, Char16,
};

use crate::{disk_helpers::*, kernel_loading::Kernel, *};

pub enum Program {
    HELP,
    EXIT,
    LS,
    CLEAR,
    PRINTMMAP,
    CD,
    RUNEFI,
    RUNKERNEL,
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
            Program::PRINTMMAP => "printmmap",
            Program::CD => "cd",
            Program::RUNEFI => "runefi",
            Program::RUNKERNEL => "runkernel",
        })
    }

    pub fn from(string: &CString16) -> Option<Program> {
        match string {
            _ if string.eq_str_until_nul("help") => return Some(Program::HELP),
            _ if string.eq_str_until_nul("exit") => return Some(Program::EXIT),
            _ if string.eq_str_until_nul("ls") => return Some(Program::LS),
            _ if string.eq_str_until_nul("clear") => return Some(Program::CLEAR),
            _ if string.eq_str_until_nul("printmmap") => return Some(Program::PRINTMMAP),
            _ if string.eq_str_until_nul("cd") => return Some(Program::CD),
            _ if string.eq_str_until_nul("runefi") => return Some(Program::RUNEFI),
            _ if string.eq_str_until_nul("runkernel") => return Some(Program::RUNKERNEL),
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
            Program::PRINTMMAP => self.print_mmap(shell),
            Program::CD => self.cd(shell),
            Program::RUNEFI => self.run_efi(shell),
            Program::RUNKERNEL => self.run_kernel(shell),
        }
    }

    fn help(&mut self, shell: &mut Shell) {
        shell.println("This is the Yannik & Mathis boot shell :)\nAvailable commands are:");
        for program in [
            Program::HELP,
            Program::EXIT,
            Program::CD,
            Program::LS,
            Program::CLEAR,
            Program::PRINTMMAP,
            Program::RUNEFI,
            Program::RUNKERNEL,
        ] {
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

        match &mut shell.fs_handle {
            None => {
                if param.is_empty() {
                    shell.println("The following FAT volumes are available:");
                    for volume_name in disk_helpers::get_volume_names(boot_services()) {
                        shell.print("- ");
                        shell.println(&volume_name);
                    }
                } else {
                    let path = Path::new(param);
                    let mut path_components = path.components();

                    if let Some(volume_name) = path_components.next() {
                        let mut result;

                        if let Some(mut fs) =
                            disk_helpers::open_volume_by_name(boot_services(), &volume_name)
                        {
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
            Some(fs_handle) => {
                if let Some(mut fs) = open_fs_handle(boot_services(), fs_handle) {
                    let s = Command::ls_fs(&mut fs, &shell.cwd, &param);
                    println!("{}", s);
                } else {
                    println!("Error: Failed to open FS handle!");
                }
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
        if let Err(err) = system_table().stdout().clear() {
            shell.print("Could not clear shell because of error: ");
            shell.debug_println(&err);
        }
    }

    fn exit(&mut self, shell: &mut Shell) {
        shell.exit = true;
    }

    fn print_mmap(&mut self, shell: &mut Shell) {
        memory::print_memory_map(boot_services()); // ugly because it does not print using the shell
    }

    fn cd(&mut self, shell: &mut Shell) {
        if self.args.len() != 1 {
            shell.println("cd needs one argument");
            return;
        }
        let param = &self.args[0];

        match &shell.fs_handle {
            Some(fs_handle) => {
                if *param == CString16::try_from("..").unwrap()
                    && shell.cwd == CString16::try_from("\\").unwrap()
                {
                    shell.fs_handle = None;
                    shell.cwd = CString16::new();
                } else {
                    let joined_path_string = Command::joined_paths(&shell.cwd, param);
                    let joined_path = Path::new(&joined_path_string);

                    if let Some(mut fs) = open_fs_handle(boot_services(), &fs_handle) {
                        match fs.read_dir(Path::new(&joined_path)) {
                            Ok(_) => {
                                shell.cwd = joined_path_string;
                            }
                            Err(err) => {
                                println!("Could not open dir because of error: {}", err);
                            }
                        }
                    } else {
                        println!("Failed to open FS handle!");
                        shell.fs_handle = None;
                        shell.cwd = CString16::new();
                    }
                }
            }
            None => {
                if let Some((volume_name, rest_path)) = Command::parse_full_path(param) {
                    shell.cwd = CString16::new();
                    shell.fs_handle = None;

                    let bs = boot_services();
                    let fs_handle = fs_handle_by_name(bs, &volume_name);

                    match fs_handle {
                        Some(fs_handle) => {
                            if let Some(mut fs) = open_fs_handle(bs, &fs_handle) {
                                let mut pathbuf = PathBuf::new();
                                pathbuf.push(Path::new(&rest_path));
                                let path = Path::new(pathbuf.to_cstr16());

                                match fs.read_dir(path) {
                                    Ok(_) => {
                                        shell.cwd = rest_path;
                                        shell.fs_handle = Some(fs_handle);
                                    }
                                    Err(err) => {
                                        println!("Could not open dir because of error: {}", err);
                                    }
                                }
                            } else {
                                println!("Failed to open FS handle!");
                            }
                        }
                        None => {
                            println!("Failed to get handle for FS with name {}", volume_name);
                        }
                    }
                } else {
                    println!("Could not parse volume name and path from: {}", param);
                }
            }
        }
    }

    pub fn run_efi(&mut self, shell: &mut Shell) {
        if self.args.len() != 1 {
            shell.println("run_efi needs one argument");
            return;
        }
        let param = &self.args[0];
        let bs = boot_services();

        match &shell.fs_handle {
            Some(fs_handle) => {
                let joined_path_string = Command::joined_paths(&shell.cwd, param);
                let joined_path = Path::new(&joined_path_string);

                if let Some(device_path) = get_device_path_for_file(
                    bs,
                    &fs_handle,
                    &joined_path.to_cstr16().try_into().unwrap(),
                ) {
                    start_efi(image_handle(), bs, &device_path);
                } else {
                    println!(
                        "Could not get device path for file path {}",
                        joined_path_string
                    );
                }
            }
            None => {
                if let Some((volume_name, rest_path)) = Command::parse_full_path(param) {
                    let fs_handle = fs_handle_by_name(bs, &volume_name);

                    match fs_handle {
                        Some(fs_handle) => {
                            let mut pathbuf = PathBuf::new();
                            pathbuf.push(Path::new(&rest_path));
                            let path = Path::new(pathbuf.to_cstr16());

                            if let Some(device_path) = get_device_path_for_file(
                                bs,
                                &fs_handle,
                                &path.to_cstr16().try_into().unwrap(),
                            ) {
                                start_efi(&image_handle(), bs, &device_path);
                            } else {
                                println!(
                                    "Could not get device path for file path {}",
                                    path.to_cstr16()
                                );
                            }
                        }
                        None => {
                            println!("Failed to get handle for FS with name {}", volume_name);
                        }
                    }
                } else {
                    println!("Could not parse volume name and path from: {}", param);
                }
            }
        }
    }

    pub fn run_kernel(&mut self, shell: &mut Shell) {
        if self.args.len() != 2 {
            shell.println("runkernel needs two arguments");
            return;
        }
        let kernel_path = &self.args[0];
        let kernel_cmdline = &self.args[1];

        match &shell.fs_handle {
            Some(fs_handle) => {
                let joined_path_string = Command::joined_paths(&shell.cwd, kernel_path);

                Kernel::load_and_start(kernel_cmdline, fs_handle, &joined_path_string);
            }
            None => {
                if let Some((volume_name, rest_path)) = Command::parse_full_path(kernel_path) {
                    let fs_handle = fs_handle_by_name(boot_services(), &volume_name);

                    match fs_handle {
                        Some(fs_handle) => {
                            let mut pathbuf = PathBuf::new();
                            pathbuf.push(Path::new(&rest_path));
                            let path = Path::new(pathbuf.to_cstr16());

                            Kernel::load_and_start(
                                kernel_cmdline,
                                &fs_handle,
                                &path.to_cstr16().try_into().unwrap(),
                            );
                        }
                        None => {
                            println!("Failed to get handle for FS with name {}", volume_name);
                        }
                    }
                } else {
                    println!("Could not parse volume name and path from: {}", kernel_path);
                }
            }
        }
    }

    pub fn parse_full_path(string: &CString16) -> Option<(CString16, CString16)> {
        let mut pathbuf = PathBuf::new();
        pathbuf.push(Path::new(string));
        let path = Path::new(pathbuf.to_cstr16());

        let mut path_components = path.components();

        if let Some(volume_name) = path_components.next() {
            let mut rest_path = CString16::new();

            for component in path_components {
                rest_path.push(Char16!('\\'));
                rest_path.push_str(&component);
            }

            Some((volume_name, rest_path))
        } else {
            None
        }
    }

    pub fn joined_paths(a: &CString16, b: &CString16) -> CString16 {
        let mut pathbuf = PathBuf::new();
        pathbuf.push(Path::new(a));
        let mut pathbuf_b = PathBuf::new();
        pathbuf_b.push(Path::new(b));

        for component in pathbuf_b.components() {
            if component == CString16!("..") {
                pathbuf = match pathbuf.parent() {
                    Some(p) => p,
                    None => PathBuf::new(),
                }
            } else if component != CString16!("")
                && component != CString16!(" ")
                && component != CString16!(".")
            {
                pathbuf.push(Path::new(&component));
            }
        }
        if pathbuf.components().count() == 0 {
            return CString16::try_from("\\").unwrap();
        }

        let mut output = CString16::new();

        for component in pathbuf.components() {
            if !component.is_empty() && component != CString16!("\\") {
                output.push(Char16!('\\'));
                output.push_str(&component);
            }
        }
        output
    }
}
