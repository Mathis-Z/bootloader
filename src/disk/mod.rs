extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec, string::String};
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::proto::device_path::{DevicePathNode, DeviceSubType, DeviceType};

use crate::simple_error::{simple_error, SimpleResult};
use crate::QuickstartOption;
use ext4_view::{Ext4, Ext4Read};
use fs::{Filesystem, FsPath};
use uefi::boot::{self, OpenProtocolParams, ScopedProtocol};
use uefi::proto::media::disk::DiskIo;
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

pub fn read_file(path: &FsPath) -> SimpleResult<Vec<u8>> {
    let Some(partition_name) = path.components.first() else {
        return simple_error!("/ is not a file.");
    };

    let partition = Partition::find_by_name(partition_name)?;

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

pub enum StorageDevice {
    Drive {
        linux_name: String,
        medium: Medium,
        partitions: Vec<Partition>,
    },
    CdRom {
        linux_name: String,
        medium: Medium,
    }
}

// while statics are not ideal this should be fine since we are single-threaded and it makes for cleaner code
static mut STORAGE_DEVICES: Option<(Vec<Handle>, Vec<StorageDevice>)> = None;

impl StorageDevice {
    pub fn linux_name(&self) -> &str {
        match self {
            StorageDevice::Drive { linux_name, .. } => linux_name.as_str(),
            StorageDevice::CdRom { linux_name, .. } => linux_name.as_str(),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            StorageDevice::Drive { medium, .. } => medium.size,
            StorageDevice::CdRom { medium, .. } => medium.size,
        }
    }

    pub fn all() -> SimpleResult<&'static mut Vec<StorageDevice>> {
        let block_handles = boot::find_handles::<BlockIO>().unwrap();
        let mut devices_changed = false;

        if let Some((old_handles, devices)) = unsafe { STORAGE_DEVICES.as_mut() } {
            if *old_handles == *block_handles { // this assumes the order of handles will not change between calls
                return Ok(devices);
            } else {
                devices_changed = true;
            }
        }

        let mut drives: Vec<StorageDevice> = Vec::new();
        let handle_dpath_pairs: Vec<(Handle, Box<DevicePath>)> = get_device_paths_for_handles(block_handles.clone());
        // TODO: sort block devices based on their DevicePaths; now we just assume that the drives will be found in the same order as linux finds (and names) them
    
        let mut sdx_devices = 0u8;  // devices that will be named like /dev/sdX under linux
        let mut nvme_devices = 0u8; // named like /dev/nvmeXnY
        let mut cd_devices = 0u8;   // named like /dev/srX
    
        // each group becomes one StorageDevice
        for group in group_block_devices(handle_dpath_pairs) {
            // finding the handle_dpath pair that is for the entire storage device (all partitions)
            // assumption: there will only ever be one of these
            let Some((root_handle, root_dpath)) = group.iter().find(|(_, dpath)| !dpath.is_partition()) else {
                println!("Found a group of block device paths that did not contain a root device path. Skipping.");
                continue;
            };
    
            // determining linux_name
            #[derive(Debug)]
            enum DriveType {
                Sdx,    // anything that will be named like /dev/sdX under linux
                Nvme { namespace: u32 },
                Cd,
            }
    
            let drive_type = if let Ok(uefi::proto::device_path::DevicePathNodeEnum::MessagingNvmeNamespace(namespace)) = root_dpath.node_after_pci_node().unwrap().as_enum() {
                DriveType::Nvme { namespace: namespace.namespace_identifier()}
            } else if group.iter().any(|(_, dpath)| dpath.contains((DeviceType::MEDIA, DeviceSubType::MEDIA_CD_ROM))) {
                DriveType::Cd
            } else {
                DriveType::Sdx
            };
    
            let scoped_prot = open_protocol_unsafe::<BlockIO>(*root_handle).unwrap();
            let root_block_io = scoped_prot.get().unwrap();
            let root_media = root_block_io.media();
    
            let root_medium = Medium {
                handle: *root_handle,
                media_id: root_media.media_id(),
                size: root_media.last_block() * (root_media.block_size() as u64), // is this correct?
            };
    
            if let DriveType::Cd = drive_type {
                drives.push(StorageDevice::CdRom { medium: root_medium, linux_name: format!("sr{}", cd_devices) });
                cd_devices += 1;
                // there are no partitions on CD drives; skip to next drive
                // actually, in testing my CD hat multiple handles which might be important but for now let's just ignore that
                continue;
            }
    
            // collect all partitions before creating the drive
            let mut partitions = Vec::new();
    
            for (handle, dpath) in group {
                // if the device path is a partition it should have a MEDIA_HARD_DRIVE node
                if let Some(uefi::proto::device_path::DevicePathNodeEnum::MediaHardDrive(harddrive)) = dpath.get_node((DeviceType::MEDIA, DeviceSubType::MEDIA_HARD_DRIVE)).and_then(|node| node.as_enum().ok()) {
                    
                    let scoped_prot = open_protocol_unsafe::<BlockIO>(handle).unwrap();
                    let block_io = scoped_prot.get().unwrap();
                    let media = block_io.media();
    
                    let medium = Medium {
                        handle,
                        media_id: media.media_id(),
                        size: media.last_block() * (media.block_size() as u64), // TODO: is this correct?
                    };
    
                    partitions.push(Partition {
                        linux_name: match drive_type {
                            DriveType::Sdx => format!("sd{}{}", ('a' as u8 + sdx_devices) as char, harddrive.partition_number()),
                            DriveType::Nvme { namespace } => format!("nvme{}n{}p{}", nvme_devices, namespace, harddrive.partition_number()),
                            DriveType::Cd => unreachable!(),
                        },
                        fs: medium.open_fs(),
                        medium,
                    });
                }
            }
    
            drives.push(StorageDevice::Drive {
                linux_name: match drive_type {
                    DriveType::Sdx => format!("sd{}", ('a' as u8 + sdx_devices) as char),
                    DriveType::Nvme { namespace } => format!("nvme{}n{}", nvme_devices, namespace),
                    DriveType::Cd => unreachable!(),
                },
                medium: root_medium,
                partitions,
            });
    
            match drive_type {
                DriveType::Sdx => sdx_devices += 1,
                DriveType::Nvme { .. } => nvme_devices += 1,
                DriveType::Cd => unreachable!(),
            }
        }

        let devices = unsafe {
            STORAGE_DEVICES = Some((block_handles, drives));
            &mut STORAGE_DEVICES.as_mut().unwrap().1
        };

        if devices_changed {
            simple_error!("The block devices have changed. The names of existing devices may have changed as well. Please check 'lsblk'.")
        } else {
            Ok(devices)
        }
    }
}

// helper for StorageDevice::all()
fn get_device_paths_for_handles(handles: Vec<Handle>) -> Vec<(Handle, Box<DevicePath>)> {
    let mut device_paths = Vec::new();
    for handle in handles {
        let device_path = open_protocol_unsafe::<DevicePath>(handle).unwrap();
        device_paths.push((handle, device_path.get().unwrap().to_boxed()));
    }
    device_paths
}

// helper for StorageDevice::all()
pub fn group_block_devices(handle_dpath_pairs: Vec<(Handle, Box<DevicePath>)>) -> Vec<Vec<(Handle, Box<DevicePath>)>> {
    let mut groups: Vec<Vec<(Handle, Box<DevicePath>)>> = Vec::new();
    let mut grouping_nodes: Vec<CString16> = Vec::new();

    for (handle, dpath) in handle_dpath_pairs {
        let node= {
            let node = dpath.node_after_pci_node().expect("A block device was not connected to a PCI device :(");

            // DevicePathNode won't let me clone it so just convert it to a string :/
            // this is a bit hacky because it assumes that all important data is contained in the string representation of the node
            node.to_string(DisplayOnly(false), AllowShortcuts(false)).unwrap()
        };
        match grouping_nodes.iter().position(|n| *n == node) {
            Some(idx) => groups[idx].push((handle, dpath)),
            None => {
                grouping_nodes.push(node);
                groups.push(alloc::vec![(handle, dpath)]);
            }
        }
    }

    groups
}

impl fmt::Display for StorageDevice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} size: {}",
            self.linux_name(),
            human_readable_size(self.size())
        )
    }
}

pub struct Partition {
    linux_name: String,
    medium: Medium,
    fs: Option<Box<dyn Filesystem>>,
}

impl Partition {
    pub fn linux_name(&self) -> &str {
        self.linux_name.as_str()
    }

    pub fn all() -> SimpleResult<Vec<&'static mut Partition>> {
        let mut partitions = Vec::new();
        for storage_device in StorageDevice::all()? {
            let StorageDevice::Drive { partitions: device_partitions, .. } = storage_device else {
                continue; // ignore CD drives
            };

            partitions.extend(device_partitions);
        }
        Ok(partitions)
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

    pub fn find_by_name(name: &str) -> SimpleResult<&mut Partition> {
        for storage_device in StorageDevice::all()? {
            let StorageDevice::Drive { partitions, .. } = storage_device else {
                continue; // ignore CD drives
            };

            for partition in partitions {
                if partition.linux_name() == name {
                    return Ok(partition);
                }
            }
        }
        simple_error!("No partition with the name {name} was found.")
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
pub fn find_quickstart_options() -> SimpleResult<Vec<QuickstartOption>> {
    let mut quickstart_options: Vec<QuickstartOption> = Vec::new();

    for storage_device in StorageDevice::all()? {
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

    Ok(quickstart_options)
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


// This is safe assuming this bootloader is the only application running and it does not conflict with itself.
pub fn open_protocol_unsafe<P>(handle: Handle) -> uefi::Result<ScopedProtocol<P>>
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


trait DevicePathConvenience {
    fn node_after_pci_node(&self) -> Option<&DevicePathNode>;
    fn is_partition(&self) -> bool;
    fn contains(&self, full_type: (DeviceType, DeviceSubType)) -> bool;
    fn get_node(&self, full_type: (DeviceType, DeviceSubType)) -> Option<&DevicePathNode>;
}

impl DevicePathConvenience for DevicePath {
    fn node_after_pci_node(&self) -> Option<&DevicePathNode> {
        self.node_iter().nth(2) // Assuming there will always be a PciRoot node followed by a PCI node
    }

    fn get_node(&self, full_type: (DeviceType, DeviceSubType)) -> Option<&DevicePathNode> {
        let mut iter = self.node_iter();
    
        for node in &mut iter {
            if node.full_type() == full_type {
                return Some(node);
            }
        }
        None
    }
    
    fn is_partition(&self) -> bool {
        self.contains((DeviceType::MEDIA, DeviceSubType::MEDIA_HARD_DRIVE))
    }
    
    fn contains(&self, full_type: (DeviceType, DeviceSubType)) -> bool {
        self.get_node(full_type).is_some()
    }
}
