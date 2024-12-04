extern crate alloc;

use alloc::vec::Vec;

use uefi::{
    print, println,
    proto::console::text::{Key, ScanCode},
    CString16, Char16,
};

use crate::{
    disk::{
        fs::{FileError, FsPath},
        Drive, Partition,
    },
    simple_error::{simple_error, SimpleResult},
};

#[macro_export]
macro_rules! Char16 {
    ($a:expr) => {{
        Char16::try_from($a).unwrap()
    }};
}

pub struct Shell {
    cmd_history_idx: usize,
    cmd_history: Vec<CString16>,
    cwd: FsPath,
    pub exit: bool,
}

impl Shell {
    pub fn new() -> Shell {
        Shell {
            cwd: FsPath::new(),
            cmd_history_idx: 0,
            cmd_history: Vec::new(),
            exit: false,
        }
    }

    pub fn enter(&mut self) {
        let _ = self.help();

        let _ = uefi::system::with_stdout(|stdout| stdout.enable_cursor(true));

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
            let key = uefi::system::with_stdin(|stdin| stdin.read_key().expect("Expected input"));
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
                                    print!("{char}");
                                }
                            }
                        }
                        Key::Special(ScanCode::DOWN) => {
                            if self.cmd_history_idx < self.cmd_history.len() - 1 {
                                self.cmd_history_idx = self.cmd_history_idx.saturating_add(1);

                                self.clear_shell_line(line.len());
                                line = Vec::new();

                                for char in self.cmd_history[self.cmd_history_idx].iter() {
                                    line.push(*char);
                                }

                                for char in &line {
                                    print!("{char}");
                                }
                            }
                        }

                        Key::Printable(key) => {
                            if key == '\r' {
                                print!("\r\n");
                                let mut s = CString16::new();
                                for char in line {
                                    s.push(char)
                                }
                                return s;
                            } else if key == '\x08' {
                                if line.pop() != None {
                                    print!("{key}");
                                }
                            } else {
                                print!("{key}");
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

    pub fn print_shell(&mut self) {
        print!("{}>> ", &self.cwd);
    }

    pub fn clear_shell_line(&mut self, chars_to_clear: usize) {
        for _ in 0..chars_to_clear {
            print!("\x08");
        }
    }

    pub fn execute_command_string(&mut self, command: CString16) {
        if !command.is_empty() {
            self.cmd_history.push(command.clone());
        }
        if let Some((program, args)) = self.parse_command(&command) {
            if let Err(error) = match alloc::string::ToString::to_string(&program).as_str() {
                "help" => self.help(),
                "exit" => self.exit(),
                "ls" => self.ls(args),
                "clear" => self.clear(),
                "printmmap" => self.print_mmap(),
                "cd" => self.cd(args),
                "runefi" => self.run_efi(args),
                "runkernel" => self.run_kernel(args),
                _ => simple_error!("Unknown command '{program}'"),
            } {
                println!("{error}");
            }
        }
    }

    pub fn parse_command(&self, command: &CString16) -> Option<(CString16, Vec<CString16>)> {
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
            let program = cmd_parts.remove(0);

            Some((program, cmd_parts))
        }
    }

    fn help(&mut self) -> SimpleResult<()> {
        println!("This is the Yannik & Mathis boot shell :)\nAvailable commands are:");
        println!("- help");
        println!("- exit");
        println!("- cd [PATH]");
        println!("- ls [PATH]");
        println!("- clear");
        println!("- printmmap");
        println!("- runefi [PATH]");
        println!("- runkernel [PATH] [KERNEL-CMDLINE]");

        Ok(())
    }

    fn ls(&mut self, args: Vec<CString16>) -> SimpleResult<()> {
        let mut path = self.cwd.clone();

        if args.len() > 1 {
            return simple_error!("ls needs 0 or 1 arguments");
        } else if args.len() == 1 {
            path.push(&args[0]);
        }

        if let Some(partition_name) = path.components.first() {
            let Some(partition) = Partition::find_by_name(partition_name) else {
                return simple_error!("No partition with the name {partition_name} was found.");
            };

            let Some(fs) = partition.fs() else {
                return simple_error!("The partition's filesystem could not be read.");
            };

            match fs.read_directory(path.path_on_partition()) {
                Err(FileError::NotADirectory) => simple_error!("{path} is not a directory"),
                Err(FileError::NotFound) => simple_error!("{path} not found."),
                Err(_) => simple_error!("An error occurred."),
                Ok(directory) => {
                    for file in directory.files() {
                        println!("{file}");
                    }
                    Ok(())
                }
            }
        } else {
            for drive in Drive::all() {
                for partition in &drive.partitions {
                    println!("{partition}")
                }
            }
            Ok(())
        }
    }

    fn clear(&mut self) -> SimpleResult<()> {
        let _ = uefi::system::with_stdout(|stdout| stdout.clear());
        Ok(())
    }

    fn exit(&mut self) -> SimpleResult<()> {
        self.exit = true;
        Ok(())
    }

    fn print_mmap(&mut self) -> SimpleResult<()> {
        crate::mem::print_memory_map();
        Ok(())
    }

    fn cd(&mut self, args: Vec<CString16>) -> SimpleResult<()> {
        if args.len() != 1 {
            return simple_error!("cd needs one argument");
        }

        let mut path = self.cwd.clone();
        path.push(&args[0]);

        let Some(partition_name) = path.components.first() else {
            self.cwd = FsPath::new();
            return Ok(());
        };

        let Some(partition) = Partition::find_by_name(partition_name) else {
            return simple_error!("No partition with the name {partition_name} was found.");
        };

        let Some(fs) = partition.fs() else {
            return simple_error!("The partition's filesystem could not be read.");
        };

        match fs.read_directory(path.path_on_partition()) {
            Err(FileError::NotADirectory) => simple_error!("{path} is not a directory"),
            Err(FileError::NotFound) => simple_error!("{path} not found."),
            Err(_) => simple_error!("An error occurred."),
            Ok(_) => {
                self.cwd = path;
                Ok(())
            }
        }
    }

    pub fn run_efi(&mut self, args: Vec<CString16>) -> SimpleResult<()> {
        if args.len() != 1 {
            return simple_error!("run_efi needs one argument");
        }

        let mut path = self.cwd.clone();
        path.push(&args[0]);

        let Some(partition_name) = path.components.first().cloned() else {
            return simple_error!("/ is not an EFI -.-");
        };

        let Some(partition) = Partition::find_by_name(&partition_name) else {
            return simple_error!("No partition with the name {partition_name} was found.");
        };

        let Some(fs) = partition.fs() else {
            return simple_error!("The partition's filesystem could not be read.");
        };

        match fs.read_file(path.path_on_partition()) {
            Err(FileError::NotAFile) => simple_error!("{path} is not a file"),
            Err(FileError::NotFound) => simple_error!("{path} not found."),
            Err(_) => simple_error!("An error occurred."),
            Ok(data) => {
                let file_dpath = partition.device_path_for_file(&path.into());

                if file_dpath.is_none() {
                    println!("Could not get device path for the file. Starting the EFI might work anyway.");
                }

                match uefi::boot::load_image(
                    uefi::boot::image_handle(),
                    uefi::boot::LoadImageSource::FromBuffer {
                        buffer: data.as_slice(),
                        file_path: file_dpath.as_deref(),
                    },
                ) {
                    Ok(loaded_image) => {
                        println!("Starting image...\n\n");
                        uefi::boot::stall(1_500_000); // time to read logs

                        if let Err(err) = uefi::boot::start_image(loaded_image) {
                            return simple_error!("Could not start EFI because of an error: {err}");
                        } else {
                            println!("The EFI application exited");
                        }
                    }
                    Err(err) => {
                        return simple_error!(
                            "Failed to load EFI image into buffer because of: {err}"
                        )
                    }
                }

                Ok(())
            }
        }
    }

    pub fn run_kernel(&mut self, args: Vec<CString16>) -> SimpleResult<()> {
        if args.len() != 2 {
            return simple_error!("runkernel needs two arguments");
        }

        let mut path = self.cwd.clone();
        path.push(&args[0]);
        let kernel_cmdline = &args[1];

        let Some(partition_name) = path.components.first() else {
            return simple_error!("/ is not a kernel -.-");
        };

        let Some(partition) = Partition::find_by_name(partition_name) else {
            return simple_error!("No partition with the name {partition_name} was found.");
        };

        let Some(fs) = partition.fs() else {
            return simple_error!("The partition's filesystem could not be read.");
        };

        match fs.read_file(path.path_on_partition()) {
            Err(FileError::NotAFile) => simple_error!("{path} is not a file"),
            Err(FileError::NotFound) => simple_error!("{path} not found."),
            Err(_) => simple_error!("An error occurred."),
            Ok(data) => {
                crate::kernel::Kernel::new(data)?.start(kernel_cmdline);
                Ok(())
            }
        }
    }
}
