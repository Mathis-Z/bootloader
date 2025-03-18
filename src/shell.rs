/*
This file contains the shell implementation. The shell calls all the other code so it is the root of the code. 
It reads and parses the keyboard input and executes the commands which are plain Rust functions.
Paths include the partitions they are on so we simulate all partitions being "mounted" at
the root directory with the same name as the partition, e.g. /sda1 /sda2 /sdb1 etc.
*/

extern crate alloc;

use alloc::{vec::Vec, string::String, string::ToString};

use uefi::{
    print, println,
    proto::{console::text::{Key, ScanCode}, BootPolicy},
};
use regex::Regex;

use crate::{
    disk::{
        fs::{FileError, FsPath}, Storage, StorageDevice
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
    cmd_history: Vec<String>,
    cwd: FsPath,
    exit: bool,
    quickstart_options: Vec<QuickstartOption>,
    storage: Storage
}

// chainloading .efi or loading a linux kernel
pub enum QuickstartOption {
    EFI { full_path: FsPath },
    Kernel { kernel_path: FsPath, cmdline: String, ramdisk_path: Option<FsPath> },
}

impl Shell {
    pub fn new() -> Shell {
        let mut shell = Shell {
            cwd: FsPath::new(),
            cmd_history_idx: 0,
            cmd_history: Vec::new(),
            exit: false,
            quickstart_options: Vec::new(),
            storage: Storage::new().expect("Could not initialize storage"),
        };

        shell.quickstart_options = shell.find_quickstart_options().unwrap_or_else(|_| Vec::new());
        shell
    }

    pub fn enter(&mut self) {
        let _ = self.help();
        println!();
        let _ = self.quickstart_options();

        let _ = uefi::system::with_stdout(|stdout| stdout.enable_cursor(true));

        // REPL loop
        while !self.exit {
            self.print_shell();
            let line = self.read_line();
            self.execute_command_string(&line);
        }
    }

    pub fn read_line(&mut self) -> String {
        let mut line = Vec::<char>::new();

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

                                for char in self.cmd_history[self.cmd_history_idx].chars() {
                                    line.push(char);
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

                                for char in self.cmd_history[self.cmd_history_idx].chars() {
                                    line.push(char);
                                }

                                for char in &line {
                                    print!("{char}");
                                }
                            }
                        }

                        Key::Printable(key) => {
                            let key = match key.try_into() {
                                Ok(key) => key,
                                Err(_) => continue, // ignore characters not representable as char
                            };

                            if key == '\r' {
                                print!("\r\n");
                                let mut s = String::new();
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
            print!("\x08"); // backspace
        }
    }

    pub fn execute_command_string(&mut self, command: &str) {
        if !command.is_empty() {
            self.cmd_history.push(command.to_string());
        }
        if let Some((program, args)) = self.parse_command(command) {
            if let Err(error) = match alloc::string::ToString::to_string(&program).as_str() {
                "help" => self.help(),
                "exit" => self.exit(),
                "ls" => self.ls(args),
                "clear" => self.clear(),
                "printmmap" => self.print_mmap(),
                "cd" => self.cd(args),
                "runefi" => self.run_efi(args),
                "runkernel" => self.run_kernel(args),
                "quickstart" => self.quickstart(args),
                "quickstart_options" => self.quickstart_options(),
                _ => simple_error!("Unknown command '{program}'"),
            } {
                println!("{error}");
            }
        }
    }

    // this is just best-effort parsing so it's probably broken is some edge cases
    pub fn parse_command(&self, command: &str) -> Option<(String, Vec<String>)> {
        let mut cmd_parts = Vec::<String>::new();
        let mut new_cmd_part = String::new();
        let mut escaped = false;
        let mut single_quoted = false;
        let mut double_quoted = false;

        for character in command.chars() {
            if character == '\\' {
                if escaped {
                    new_cmd_part.push(character);
                }
                escaped = !escaped;
            } else if character == ' ' {
                if escaped {
                    return None;
                } else {
                    if single_quoted || double_quoted {
                        new_cmd_part.push(character);
                    } else if !new_cmd_part.is_empty() {
                        cmd_parts.push(new_cmd_part);
                        new_cmd_part = String::new();
                    }
                }
            } else if character == '\'' {
                if escaped || double_quoted {
                    new_cmd_part.push(character);
                } else {
                    single_quoted = !single_quoted;
                }
            } else if character == '\"' {
                if escaped || single_quoted {
                    new_cmd_part.push(character);
                } else {
                    double_quoted = !double_quoted;
                }
            } else {
                new_cmd_part.push(character)
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

    // for simplicity, all commands return SimpleResult<()>
    fn help(&mut self) -> SimpleResult<()> {
        println!("This is the Yannik & Mathis boot shell :)\nAvailable commands are:");
        println!("- help");
        println!("- exit");
        println!("- cd [PATH]");
        println!("- ls [PATH]");
        println!("- clear");
        println!("- printmmap");
        println!("- runefi [PATH]");
        println!("- runkernel [PATH] [KERNEL-CMDLINE] [opt. RAMDISK]");
        println!("- quickstart_options");
        println!("- quickstart [IDX]");

        Ok(())
    }

    // search all partitions for linux kernel images or the windows bootloader .efi
    pub fn find_quickstart_options(&mut self) -> SimpleResult<Vec<QuickstartOption>> {
        let mut quickstart_options: Vec<QuickstartOption> = Vec::new();

        for storage_device in self.storage.devices()? {
            let StorageDevice::Drive { partitions, .. } = storage_device else {
                continue; // ignore CD drives
            };

            for partition in partitions {
                let partition_name = partition.linux_name().to_string();
                let Some(fstype) = partition.fstype() else {
                    continue;   // Cannot read 'Unknown' filesystems anyway
                };

                let Some(fs) = partition.fs() else {
                    continue;
                };

                if fstype == crate::disk::fs::FsType::Fat {
                    const WINDOWS_EFI_PATH: &str = "/EFI/Microsoft/Boot/bootmgfw.efi";

                    if let Ok(_) = fs.read_file(WINDOWS_EFI_PATH) {
                        let full_path = FsPath::parse(alloc::format!("/{partition_name}{WINDOWS_EFI_PATH}")).unwrap();

                        quickstart_options.push(QuickstartOption::EFI { full_path })
                    }
                }

                for directory_to_search in alloc::vec!["/", "/boot"] {
                    let Ok(dir) = fs.read_directory(directory_to_search) else {
                        continue;
                    };

                    let cwd = FsPath::parse(alloc::format!("/{partition_name}{directory_to_search}")).unwrap();
                    let files = dir.files();

                    // For simplicity we assume that kernel image names will be like vmlinuz-<version> or bzImage-<version>
                    // Otherwise the user has to go find their kernel image themself >:/
                    let kernel_regex = Regex::new(r"^(vmlinuz|bzImage)-(.+)$").unwrap();
                    let ramdisk_regex = Regex::new(r"^(initrd\.img|initramfs)-(.+)(\.img)?$").unwrap();

                    let mut kernels = alloc::collections::btree_map::BTreeMap::new();
                    let mut ramdisks = alloc::collections::btree_map::BTreeMap::new();

                    for file in files {
                        if !file.is_regular_file() || file.size() < 1000 {
                            continue;
                        }

                        let file_name_cstring = file.name();
                        let mut file_path = cwd.clone();
                        file_path.push(&file_name_cstring);

                        let file_name = file_name_cstring.to_string();
                        
                        let kernel_match = kernel_regex.captures(&file_name);
                        let ramdisk_match = ramdisk_regex.captures(&file_name);

                        if let Some(caps) = kernel_match {
                            if let Some(version) = caps.get(2) {
                                kernels.insert(version.as_str().to_string(), file_path);
                            }
                        } else if let Some(caps) = ramdisk_match {
                            if let Some(version) = caps.get(2) {
                                ramdisks.insert(version.as_str().to_string(), file_path);
                            }
                        }
                    }

                    for (version, kernel_path) in kernels {
                        quickstart_options.push(
                            QuickstartOption::Kernel {
                                kernel_path: kernel_path.clone(),
                                ramdisk_path: ramdisks.get(&version).cloned(),
                                cmdline: alloc::format!("root=/dev/{}", partition_name)
                            }
                        );
                    }
                }
            }
        }

        Ok(quickstart_options)
    }

    fn quickstart(&mut self, args: Vec<String>) -> SimpleResult<()> {
        if args.len() != 1 {
            return simple_error!("quickstart takes one argument");
        }

        let quickstart_idx: usize = alloc::string::ToString::to_string(&args[0])
            .parse()
            .or_else(|_| simple_error!("Could not parse '{}' as integer", args[0]))?;

        match self.quickstart_options.get(quickstart_idx) {
            Some(QuickstartOption::EFI { full_path }) => {
                return self.run_efi(alloc::vec![full_path.into()]);
            },
            Some(QuickstartOption::Kernel { kernel_path, cmdline, ramdisk_path }) => {
                let mut args = Vec::new();
                args.push(kernel_path.into());
                args.push(cmdline.clone());

                if let Some(ramdisk_path) = ramdisk_path {
                    args.push(ramdisk_path.into());
                }

                return self.run_kernel(args);
            },
            None => simple_error!("{quickstart_idx} is out of range"),
        }
    }

    fn quickstart_options(&mut self) -> SimpleResult<()> {
        if self.quickstart_options.is_empty() {
            println!("No quickstart options found.");
            return Ok(());
        }

        println!(" Your quickstart options are:");

        for (idx, opt) in self.quickstart_options.iter().enumerate() {
            match opt {
                QuickstartOption::EFI { full_path } => {
                    println!("[{idx}] runefi {full_path}");
                },
                QuickstartOption::Kernel { kernel_path, cmdline, ramdisk_path } => {
                    if let Some(ramdisk_path) = &ramdisk_path {
                        println!("[{idx}] runkernel {kernel_path} '{cmdline}' {ramdisk_path}");
                    } else {
                        println!("[{idx}] runkernel {kernel_path} '{cmdline}'");
                    }
                },
            }
        }
        Ok(())
    }

    // print directory contents or partitions if executing "ls /"
    fn ls(&mut self, args: Vec<String>) -> SimpleResult<()> {
        let mut path = self.cwd.clone();

        if args.len() > 1 {
            return simple_error!("ls needs 0 or 1 arguments");
        } else if args.len() == 1 {
            path.push(&args[0]);
        }

        if let Some(partition_name) = path.components.first() {
            let partition = self.storage.partition_by_name(partition_name)?;

            let Some(fs) = partition.fs() else {
                return simple_error!("The partition's filesystem could not be read.");
            };

            match fs.read_directory(&path.path_on_partition()) {
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
            for partition in self.storage.partitions()? {
                println!("{partition}");
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

    fn cd(&mut self, args: Vec<String>) -> SimpleResult<()> {
        if args.len() != 1 {
            return simple_error!("cd needs one argument");
        }

        let mut path = self.cwd.clone();
        path.push(&args[0]);

        let Some(partition_name) = path.components.first() else {
            self.cwd = FsPath::new();
            return Ok(());
        };

        let partition = self.storage.partition_by_name(partition_name)?;

        let Some(fs) = partition.fs() else {
            return simple_error!("The partition's filesystem could not be read.");
        };

        match fs.read_directory(&path.path_on_partition()) {
            Err(FileError::NotADirectory) => simple_error!("{path} is not a directory"),
            Err(FileError::NotFound) => simple_error!("{path} not found."),
            Err(_) => simple_error!("An error occurred."),
            Ok(_) => {
                self.cwd = path;
                Ok(())
            }
        }
    }

    pub fn run_efi(&mut self, args: Vec<String>) -> SimpleResult<()> {
        if args.len() != 1 {
            return simple_error!("run_efi needs one argument");
        }

        let mut path = self.cwd.clone();
        path.push(&args[0]);

        let Some(partition_name) = path.components.first().cloned() else {
            return simple_error!("/ is not an EFI -.-");
        };

        let partition = self.storage.partition_by_name(&partition_name)?;

        let Some(fs) = partition.fs() else {
            return simple_error!("The partition's filesystem could not be read.");
        };

        println!("Loading image into memory...");
        match fs.read_file(&path.path_on_partition()) {
            Err(FileError::NotAFile) => simple_error!("{path} is not a file"),
            Err(FileError::NotFound) => simple_error!("{path} not found."),
            Err(_) => simple_error!("An error occurred."),
            Ok(_) => {
                let file_dpath = partition.device_path_for_file::<String>(path.into());

                if file_dpath.is_none() {
                    println!("Could not get device path for the file. Starting the EFI might work anyway.");
                }

                match uefi::boot::load_image(
                    uefi::boot::image_handle(),
                    uefi::boot::LoadImageSource::FromDevicePath {
                        device_path: &file_dpath.unwrap(),
                        boot_policy: BootPolicy::ExactMatch,
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

    pub fn run_kernel(&mut self, args: Vec<String>) -> SimpleResult<()> {
        if args.len() < 2 || args.len() > 3 {
            return simple_error!("runkernel needs two or three arguments");
        }

        let mut ramdisk = None;
        if args.len() == 3 {
            let mut ramdisk_image_path = self.cwd.clone();
            ramdisk_image_path.push(&args[2]);

            ramdisk = Some(self.storage.read_file(&ramdisk_image_path).or_else(|err| {
                simple_error!("Could not read ramdisk image: {err}")
            })?);

            println!("Ramdisk loaded at {:x}", ramdisk.as_ref().unwrap().as_ptr() as usize);
            println!("Ramdisk size: {:x}", ramdisk.as_ref().unwrap().len());
            
        }

        let mut kernel_image_path = self.cwd.clone();
        kernel_image_path.push(&args[0]);

        let kernel = self.storage.read_file(&kernel_image_path).or_else(|err| {
            simple_error!("Could not read kernel image: {err}")
        })?;

        let kernel_cmdline = &args[1];

        crate::kernel::Kernel::new(kernel)?.start(kernel_cmdline, ramdisk)
    }
}
