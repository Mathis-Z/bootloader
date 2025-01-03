extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec, string::String};
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::device_path::build::DevicePathBuilder;

use crate::simple_error::simple_error;
use crate::QuickstartOption;
use ext4_view::{Ext4, Ext4Read};
use fs::{Filesystem, FsPath};
use uefi::boot::{self, image_handle, OpenProtocolParams, ScopedProtocol};
use uefi::proto::media::disk::DiskIo;
use uefi::proto::media::partition::PartitionInfo;
use uefi::proto::{media::block::BlockIO, ProtocolPointer};
use uefi::CString16;
use uefi::{
    println,
    proto::{device_path::DevicePath, media::fs::SimpleFileSystem},
    Handle,
};

use crate::simple_error::{self, SimpleError};
use regex::Regex;
use alloc::string::ToString;

pub mod fs;

pub fn read_file(path: &FsPath) -> simple_error::SimpleResult<Vec<u8>> {
    let Some(partition_name) = path.components.first() else {
        return simple_error!("/ is not a file.");
    };

    let Some(partition) = Partition::find_by_name(partition_name) else {
        return simple_error!("No partition with the name {partition_name} was found.");
    };

    let Some(fs) = partition.fs() else {
        return simple_error!("The partition's filesystem could not be read.");
    };

    match fs.read_file(&path.path_on_partition()) {
        Err(fs::FileError::NotAFile) => simple_error!("{path} is not a file."),
        Err(fs::FileError::NotFound) => simple_error!("{path} not found."),
        Err(_) => simple_error!("An error occurred."),
        Ok(data) => Ok(data),
    }
}

// this assumes no drives will be connected or disconnected while the bootloader runs
// TODO: make this ugly code prettier
static mut DRIVES: Option<Vec<Drive>> = None;

pub struct Drive {
    pub idx: u8,
    pub medium: Medium,
    pub partitions: Vec<Partition>,
}

impl Drive {
    pub fn linux_name(&self) -> String {
        format!("/dev/sd{}", ('a' as u8 + self.idx) as char)
    }

    // TODO: is this the right place for this method?
    pub fn all() -> &'static mut Vec<Drive> {
        if let Some(drives) = unsafe { DRIVES.as_mut() } {
            return drives;
        }

        let block_handles = boot::find_handles::<BlockIO>().unwrap();

        let mut drives: Vec<Drive> = Vec::new();
        let mut idx: u8 = 0;

        // First we find all drives and ignore the partitions. This code assumes that the drives will be found in the same order as linux finds (and names) them.
        // This works with OVMF in QEMU and I hope it does on real machines as well.
        // The (much harder) alternative would be to sort the drives based on their DevicePaths.
        // For example a drive attached under PCIe device 2 should come after all drives attached to PCIe device 1.
        // This would be more predictable and probably match linux's naming scheme but yk
        for &handle in &block_handles {
            if !supports_protocol::<PartitionInfo>(handle) {
                let scoped_prot = open_protocol_unsafe::<BlockIO>(handle).unwrap();
                let block_io = scoped_prot.get().unwrap();

                let medium = Medium {
                    handle,
                    media_id: block_io.media().media_id(),
                    size: block_io.media().last_block() * (block_io.media().block_size() as u64), // TODO: is this correct?
                };

                drives.push(Drive {
                    idx,
                    medium,
                    partitions: Vec::new(),
                });

                idx += 1;
            }
        }

        // Now we "attach" the partitions to their drives.
        for &handle in &block_handles {
            if supports_protocol::<PartitionInfo>(handle) {
                // There is a problem in the OVMF firmware where exclusively opening a protocol on the handle for an entire drive would
                // lock the handle but the lock never gets released.
                // Since the "child handles" (the handles representing partitions on that drive) also get locked, one can never open a protocol on these handles afterwards.
                // This is why we use open_protocol_unsafe in this code.
                let scoped_prot = open_protocol_unsafe::<BlockIO>(handle).unwrap();
                let block_io = scoped_prot.get().unwrap();

                let medium = Medium {
                    handle,
                    media_id: block_io.media().media_id(),
                    size: block_io.media().last_block() * (block_io.media().block_size() as u64), // TODO: is this correct?
                };

                let Some(parent_drive) = find_matching_drive(&mut drives, handle) else {
                    println!("Found a partition without a matching drive. What is happening?");
                    continue;
                };

                let part = Partition {
                    drive_idx: parent_drive.idx,
                    idx: parent_drive.partitions.len() as u8 + 1,
                    fs: medium.open_fs(),
                    medium,
                };

                parent_drive.partitions.push(part);
            }
        }

        unsafe {
            DRIVES = Some(drives);
            return DRIVES.as_mut().unwrap();
        }
    }
}

impl fmt::Display for Drive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} size: {}",
            self.linux_name(),
            human_readable_size(self.medium.size)
        )
    }
}

pub struct Partition {
    drive_idx: u8,
    idx: u8,
    medium: Medium,
    fs: Option<Box<dyn Filesystem>>,
}

impl Partition {
    pub fn linux_name(&self) -> String {
        format!("sd{}{}", ('a' as u8 + self.drive_idx) as char, self.idx)
    }

    pub fn fstype(&self) -> Option<fs::FsType> {
        Some(self.fs.as_ref()?.format())
    }

    pub fn fstype_as_str(&self) -> &str {
        match self.fstype() {
            None => "Unknown",
            Some(fs::FsType::Ext4) => "EXT4",
            Some(fs::FsType::Fat) => "FAT",
        }
    }

    pub fn find_by_name(name: &str) -> Option<&mut Partition> {
        for drive in Drive::all() {
            for partition in &mut drive.partitions {
                if &partition.linux_name() == name {
                    return Some(partition);
                }
            }
        }

        None
    }

    pub fn fs(&mut self) -> Option<&mut Box<dyn Filesystem>> {
        self.fs.as_mut()
    }

    // TODO: is this the right place for this method?
    pub fn device_path_for_file<S: AsRef<str>>(&self, file_path_str: S) -> Option<Box<DevicePath>> {
        let file_path_str= file_path_str.as_ref();

        let mut full_dpath_buf = Vec::new();
        let mut full_dpath_builder = DevicePathBuilder::with_vec(&mut full_dpath_buf);

        for node in self.device_path()?.node_iter() {
            full_dpath_builder = full_dpath_builder.push(&node).ok()?;
        }

        let path_on_partition_str: CString16  = FsPath::parse(file_path_str).ok()?.to_uefi_string(false).ok()?;

        // appending file path node to the device path of the filesystem yields the full path
        Some(
            full_dpath_builder
                .push(&FilePath {
                    path_name: &path_on_partition_str,
                })
                .ok()?
                .finalize()
                .ok()?
                .to_boxed(),
        )
    }

    fn device_path(&self) -> Option<Box<DevicePath>> {
        let mut scoped_prot =
            uefi::boot::open_protocol_exclusive::<DevicePath>(self.medium.handle).ok()?;
        Some(scoped_prot.get_mut()?.to_boxed())
    }
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}  {}  format: {}",
            self.linux_name(),
            human_readable_size(self.medium.size),
            self.fstype_as_str(),
        )
    }
}

// TODO: Can we merge Medium into Partition?
#[derive(Clone, Copy)]
pub struct Medium {
    pub handle: Handle,
    pub media_id: u32,
    pub size: u64,
}

impl Medium {
    pub fn open_fs(self) -> Option<Box<dyn Filesystem>> {
        if let Ok(sfs) = open_protocol_unsafe::<SimpleFileSystem>(self.handle) {
            return Some(Box::new(sfs));
        }

        if let Ok(ext) = Ext4::load(Box::new(self)) {
            return Some(Box::new(ext));
        }

        None
    }
}

// search all partitions for linux kernel images or the windows bootloader .efi
pub fn find_quickstart_options() -> Vec<QuickstartOption> {
    let mut quickstart_options: Vec<QuickstartOption> = Vec::new();

    for drive in Drive::all() {
        for partition in &mut drive.partitions {
            let partition_name = partition.linux_name();
            let Some(fstype) = partition.fstype() else {
                continue;   // Cannot read 'Unknown' filesystems anyway
            };

            let Some(fs) = partition.fs() else {
                continue;
            };

            if fstype == fs::FsType::Fat {
                const WINDOWS_EFI_PATH: &str = "/EFI/Microsoft/Boot/bootmgfw.efi";

                if let Ok(_) = fs.read_file(WINDOWS_EFI_PATH) {
                    let full_path = FsPath::parse(format!("/{partition_name}{WINDOWS_EFI_PATH}")).unwrap();

                    quickstart_options.push(QuickstartOption::EFI { full_path })
                }
            }

            for directory_to_search in alloc::vec!["/", "/boot"] {
                let Ok(dir) = fs.read_directory(directory_to_search) else {
                    continue;
                };

                let cwd = FsPath::parse(format!("/{partition_name}{directory_to_search}")).unwrap();
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

    quickstart_options
}

pub fn human_readable_size(size: u64) -> String {
    const K: u64 = 1024;
    const M: u64 = 1024 * K;
    const G: u64 = 1024 * M;

    if size >= 10 * G {
        format!("{:>4} GB", size / G)
    } else if size >= 10 * M {
        format!("{:>4} MB", size / M)
    } else if size >= 10 * K {
        format!("{:>4} KB", size / K)
    } else {
        format!("{:>4} B ", size)
    }
}

fn find_matching_drive(drives: &mut Vec<Drive>, partition_handle: Handle) -> Option<&mut Drive> {
    for drive in drives {
        if partition_handle_matches_drive_handle(drive.medium.handle, partition_handle) {
            return Some(drive);
        }
    }
    None
}

fn partition_handle_matches_drive_handle(drive_handle: Handle, partition_handle: Handle) -> bool {
    let dpath = open_protocol_unsafe::<DevicePath>(drive_handle).unwrap();
    let ppath = open_protocol_unsafe::<DevicePath>(partition_handle).unwrap();
    let mut dpath_iter = dpath.node_iter();
    let mut ppath_iter = ppath.node_iter();

    loop {
        let Some(next_ppath_node) = ppath_iter.next() else {
            return false; // if ppath ends earlier than dpath the partition cannot be a subnode of the disk
        };

        match dpath_iter.next() {
            Some(dpath_node) => {
                if dpath_node != next_ppath_node {
                    return false;
                }
            }
            None => return ppath_iter.next() == None, // when dpath ends then ppath should end one node later so the partition is a direct child of its disk
        }
    }
}

// This is safe assuming this bootloader is the only application running and it does not conflict with itself.
fn open_protocol_unsafe<P>(handle: Handle) -> uefi::Result<ScopedProtocol<P>>
where
    P: ProtocolPointer + ?Sized,
{
    unsafe {
        boot::open_protocol::<P>(
            OpenProtocolParams {
                handle,
                agent: boot::image_handle(),
                controller: None,
            },
            boot::OpenProtocolAttributes::GetProtocol,
        )
    }
}

fn supports_protocol<P: ProtocolPointer>(handle: Handle) -> bool {
    boot::test_protocol::<P>(OpenProtocolParams {
        handle,
        agent: image_handle(),
        controller: None,
    })
    .unwrap()
}

impl Ext4Read for Medium {
    fn read(
        &mut self,
        start_byte: u64,
        dst: &mut [u8],
    ) -> Result<(), Box<dyn core::error::Error + Send + Sync + 'static>> {
        let disk_io = open_protocol_unsafe::<DiskIo>(self.handle).unwrap();

        match disk_io.read_disk(self.media_id, start_byte, dst) {
            Ok(()) => Ok(()),
            Err(uefi_error) => Err(Box::new(SimpleError::from(uefi_error))),
        }
    }
}
