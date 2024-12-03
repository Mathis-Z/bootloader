extern crate alloc;

use crate::disk::get_device_path_for_file;
use crate::simple_error::simple_error;
use alloc::{string::String, vec::Vec};
use boot::{image_handle, LoadImageSource};
use disk::fs::{FileError, FsPath};
use disk::{get_drives, Partition};
use simple_error::SimpleResult;
use uefi::{data_types::EqStrUntilNul, println, CString16};

use crate::{kernel::Kernel, *};

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
    pub fn execute(&mut self, shell: &mut Shell) -> SimpleResult<()> {
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

    fn help(&mut self, shell: &mut Shell) -> SimpleResult<()> {
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

        Ok(())
    }

    fn ls(&mut self, shell: &mut Shell) -> SimpleResult<()> {
        let mut path = shell.cwd.clone();

        if self.args.len() > 1 {
            return simple_error!("ls needs 0 or 1 arguments");
        } else if self.args.len() == 1 {
            path.push(&self.args[0]);
        }

        if let Some(partition_name) = path.components.first() {
            let Some(mut partition) = Partition::find_by_name(partition_name) else {
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
                        shell.println(file);
                    }
                    Ok(())
                }
            }
        } else {
            for drive in get_drives() {
                for partition in drive.partitions {
                    println!("{partition}")
                }
            }
            Ok(())
        }
    }

    fn clear(&mut self, _shell: &mut Shell) -> SimpleResult<()> {
        let _ = system::with_stdout(|stdout| stdout.clear());
        Ok(())
    }

    fn exit(&mut self, shell: &mut Shell) -> SimpleResult<()> {
        shell.exit = true;
        Ok(())
    }

    fn print_mmap(&mut self, _shell: &mut Shell) -> SimpleResult<()> {
        crate::mem::print_memory_map();
        Ok(())
    }

    fn cd(&mut self, shell: &mut Shell) -> SimpleResult<()> {
        if self.args.len() != 1 {
            return simple_error!("cd needs one argument");
        }

        let mut path = shell.cwd.clone();
        path.push(&self.args[0]);

        let Some(partition_name) = path.components.first() else {
            shell.cwd = FsPath::new();
            return Ok(());
        };

        let Some(mut partition) = Partition::find_by_name(partition_name) else {
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
                shell.cwd = path;
                Ok(())
            }
        }
    }

    pub fn run_efi(&mut self, shell: &mut Shell) -> SimpleResult<()> {
        if self.args.len() != 1 {
            return simple_error!("run_efi needs one argument");
        }

        let mut path = shell.cwd.clone();
        path.push(&self.args[0]);

        let Some(partition_name) = path.components.first() else {
            return simple_error!("/ is not an EFI -.-");
        };

        let Some(mut partition) = Partition::find_by_name(partition_name) else {
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
                let file_dpath = get_device_path_for_file(&partition.medium().handle, &path.into());

                if file_dpath.is_none() {
                    println!("Could not get device path for the file. Starting the EFI might work anyway.");
                }

                match uefi::boot::load_image(
                    image_handle(),
                    LoadImageSource::FromBuffer {
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

    pub fn run_kernel(&mut self, shell: &mut Shell) -> SimpleResult<()> {
        if self.args.len() != 2 {
            return simple_error!("runkernel needs two arguments");
        }

        let mut path = shell.cwd.clone();
        path.push(&self.args[0]);
        let kernel_cmdline = &self.args[1];

        let Some(partition_name) = path.components.first() else {
            return simple_error!("/ is not a kernel -.-");
        };

        let Some(mut partition) = Partition::find_by_name(partition_name) else {
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
                Kernel::new(data)?.start(kernel_cmdline);
                Ok(())
            }
        }
    }
}
