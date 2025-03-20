// This file contains some abstractions from the UEFI API so we don't have to work with handles or device paths in much of the other code.

extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec, string::String};
use uefi::proto::device_path::build::media::FilePath;
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::proto::device_path::{DevicePathNode, DeviceSubType, DeviceType};

use crate::simple_error::{simple_error, SimpleResult};
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

pub mod fs;

pub struct Storage {
    devices: Vec<StorageDevice>,
    last_seen_block_handles: Vec<Handle>,   // used to quickly check if we need to update the devices
}

pub enum StorageDevice {
    Drive {
        linux_name: String,
        size: u64,
        partitions: Vec<Partition>,
    },
    CdRom {
        linux_name: String,
        size: u64,
    }
}

pub struct Partition {
    linux_name: String,
    handle: Handle,
    media_id: u32,
    size: u64,
    fs: Option<Box<dyn Filesystem>>,
}

struct DiskIoMediaIdPair {
    disk_io: ScopedProtocol<DiskIo>,
    media_id: u32,
}

impl Storage {
    pub fn new() -> SimpleResult<Storage> {
        let block_handles = uefi::boot::find_handles::<BlockIO>().unwrap();
        let devices = StorageDevice::from_block_handles(&block_handles)?;
        Ok(Storage {
            devices,
            last_seen_block_handles: block_handles,
        })
    }

    pub fn devices(&mut self) -> SimpleResult<&mut Vec<StorageDevice>> {
        let block_handles = uefi::boot::find_handles::<BlockIO>().unwrap();
        let devices_changed = self.last_seen_block_handles != block_handles;

        if devices_changed {
            self.devices = StorageDevice::from_block_handles(&block_handles)?;
            self.last_seen_block_handles = block_handles;
            return simple_error!("The block devices have changed. The names of existing devices may have changed as well. Please check with 'ls'.")
        }

        Ok(self.devices.as_mut())
    }

    pub fn read_file(&mut self, path: &FsPath) -> SimpleResult<Vec<u8>> {
        let Some(partition_name) = path.components.first() else {
            return simple_error!("/ is not a file.");
        };
    
        let partition = self.partition_by_name(partition_name)?;
    
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

    pub fn partitions(&mut self) -> SimpleResult<Vec<&mut Partition>> {
        let mut partitions = Vec::new();
        for storage_device in self.devices()? {
            let StorageDevice::Drive { partitions: device_partitions, .. } = storage_device else {
                continue; // ignore CD drives
            };

            partitions.extend(device_partitions);
        }
        Ok(partitions)
    }

    pub fn partition_by_name(&mut self, name: &str) -> SimpleResult<&mut Partition> {
        for storage_device in self.devices()? {
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
}

impl StorageDevice {
    pub fn linux_name(&self) -> &str {
        match self {
            StorageDevice::Drive { linux_name, .. } => linux_name.as_str(),
            StorageDevice::CdRom { linux_name, .. } => linux_name.as_str(),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            StorageDevice::Drive { size, .. } => *size,
            StorageDevice::CdRom { size, .. } => *size,
        }
    }

    pub fn from_block_handles(block_handles: &Vec<Handle>) -> SimpleResult<Vec<StorageDevice>> {
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
            let root_size = root_media.last_block() * (root_media.block_size() as u64); // is this correct?
    
            if let DriveType::Cd = drive_type {
                drives.push(StorageDevice::CdRom { linux_name: format!("sr{}", cd_devices), size: root_size });
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

                    let partition = Partition::new(
                        match drive_type {
                            DriveType::Sdx => format!("sd{}{}", ('a' as u8 + sdx_devices) as char, harddrive.partition_number()),
                            DriveType::Nvme { namespace } => format!("nvme{}n{}p{}", nvme_devices, namespace, harddrive.partition_number()),
                            DriveType::Cd => unreachable!(),
                        },
                        handle,
                        media.media_id(),
                        media.last_block() * (media.block_size() as u64), // TODO: is this correct?
                    );

                    partitions.push(partition);
                }
            }
    
            drives.push(StorageDevice::Drive {
                linux_name: match drive_type {
                    DriveType::Sdx => format!("sd{}", ('a' as u8 + sdx_devices) as char),
                    DriveType::Nvme { namespace } => format!("nvme{}n{}", nvme_devices, namespace),
                    DriveType::Cd => unreachable!(),
                },
                size: root_size,
                partitions,
            });
    
            match drive_type {
                DriveType::Sdx => sdx_devices += 1,
                DriveType::Nvme { .. } => nvme_devices += 1,
                DriveType::Cd => unreachable!(),
            }
        }

        Ok(drives)
    }
}

// helper for StorageDevice::from_block_handes()
fn get_device_paths_for_handles(handles: Vec<Handle>) -> Vec<(Handle, Box<DevicePath>)> {
    let mut device_paths = Vec::new();
    for handle in handles {
        let device_path = open_protocol_unsafe::<DevicePath>(handle).unwrap();
        device_paths.push((handle, device_path.get().unwrap().to_boxed()));
    }
    device_paths
}

// helper for StorageDevice::from_block_handes()
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

impl Partition {
    pub fn new(
        linux_name: String,
        handle: Handle,
        media_id: u32,
        size: u64,
    ) -> Self {
        let mut partition = Partition {
            linux_name,
            handle,
            media_id,
            size,
            fs: None,
        };

        partition.fs = partition.open_fs();
        partition
    }

    pub fn open_fs(&self) -> Option<Box<dyn Filesystem>> {
        if let Ok(sfs) = open_protocol_unsafe::<SimpleFileSystem>(self.handle) {
            return Some(Box::new(sfs));
        }

        let disk_io_media_id_pair = DiskIoMediaIdPair {
            disk_io: open_protocol_unsafe::<DiskIo>(self.handle).unwrap(),
            media_id: self.media_id,
        };

        if let Ok(ext) = Ext4::load(Box::new(disk_io_media_id_pair)) {
            return Some(Box::new(ext));
        }

        None
    }

    pub fn linux_name(&self) -> &str {
        self.linux_name.as_str()
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

    pub fn fs(&mut self) -> Option<&mut Box<dyn Filesystem>> {
        self.fs.as_mut()
    }

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
            uefi::boot::open_protocol_exclusive::<DevicePath>(self.handle).ok()?;
        Some(scoped_prot.get_mut()?.to_boxed())
    }
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}  {}  format: {}",
            self.linux_name(),
            human_readable_size(self.size),
            self.fstype_as_str(),
        )
    }
}


pub fn human_readable_size(size: u64) -> String {
    const K: u64 = 1024;
    const M: u64 = 1024 * K;
    const G: u64 = 1024 * M;

    if size >= 10 * G {
        format!("{:>4} GiB", size / G)
    } else if size >= 10 * M {
        format!("{:>4} MiB", size / M)
    } else if size >= 10 * K {
        format!("{:>4} KiB", size / K)
    } else {
        format!("{:>4} B  ", size)
    }
}


// This is safe assuming this bootloader is the only application running and it does not conflict with itself.
// Unfortunately, we cannot always open a protocol safely because e.g. opening the disk protocol on the handle for an entire disk
// will lock the handles for the partitions of that disk. However, releasing the lock on the disk handle will
// not release the lock on the partition handles (so we can never open the disk protocol safely on them).
// This is probably a bug in the OVMF firmware.
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


impl Ext4Read for DiskIoMediaIdPair {
    fn read(
        &mut self,
        start_byte: u64,
        dst: &mut [u8],
    ) -> Result<(), Box<dyn core::error::Error + Send + Sync + 'static>> {
        match self.disk_io.read_disk(self.media_id, start_byte, dst) {
            Ok(()) => Ok(()),
            Err(uefi_error) => Err(Box::new(SimpleError::from(uefi_error))),
        }
    }
}

// implementing some simple helper functions for DevicePaths that are not in the UEFI crate :(
trait DevicePathConvenience {
    fn node_after_pci_node(&self) -> Option<&DevicePathNode>;
    fn is_partition(&self) -> bool;
    fn contains(&self, full_type: (DeviceType, DeviceSubType)) -> bool;
    fn get_node(&self, full_type: (DeviceType, DeviceSubType)) -> Option<&DevicePathNode>;
}

impl DevicePathConvenience for DevicePath {
    fn node_after_pci_node(&self) -> Option<&DevicePathNode> {
        self.node_iter().nth(2) // Assuming there will always be a PciRoot node followed by a PCI node; this was true for all devices I tested
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
