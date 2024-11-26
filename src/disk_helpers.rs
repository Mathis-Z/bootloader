extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec};

use ext4_view::{Ext4, Ext4Error, Ext4Read};
use uefi::boot::{self, image_handle, LoadImageSource, OpenProtocolParams, ScopedProtocol};
use uefi::fs::FileSystem;
use uefi::proto::media::disk::DiskIo;
use uefi::proto::media::partition::PartitionInfo;
use uefi::proto::{media::block::BlockIO, BootPolicy, ProtocolPointer};
use uefi::CString16;
use uefi::{
    println,
    proto::{
        device_path::{
            text::{AllowShortcuts, DevicePathFromText, DisplayOnly},
            DevicePath,
        },
        media::{
            file::{Directory, File, FileSystemVolumeLabel},
            fs::SimpleFileSystem,
        },
    },
    Handle,
};

use crate::simple_error::SimpleError;

pub fn get_volume_names() -> Vec<CString16> {
    let handles =
        uefi::boot::find_handles::<SimpleFileSystem>().expect("Failed to get FS handles!");
    let mut names = Vec::new();

    for handle in handles {
        names.push(get_volume_name(handle));
    }
    return names;
}

// TODO: Find better fallback name if volume label is empty
pub fn get_volume_name(fs_handle: Handle) -> CString16 {
    if let Ok(mut scoped_prot) = uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
    {
        if let Some(fs_protocol) = scoped_prot.get_mut() {
            if let Ok(mut root_directory) = fs_protocol.open_volume() {
                let volume_name = volume_name_from_root_dir(&mut root_directory);
                if volume_name.is_empty() {
                    return CString16::try_from(format!("{:?}", fs_handle.as_ptr()).as_str())
                        .unwrap();
                } else {
                    return volume_name;
                }
            }
        }
    }

    CString16::try_from("[Volume name error]").unwrap()
}

pub fn open_volume_by_name(name: &CString16) -> Option<FileSystem> {
    if let Some(fs_handle) = fs_handle_by_name(name) {
        return open_fs_handle(&fs_handle);
    }
    None
}

pub fn fs_handle_by_name(name: &CString16) -> Option<Handle> {
    let handles =
        uefi::boot::find_handles::<SimpleFileSystem>().expect("Failed to get FS handles!");

    for handle in handles {
        if *name == get_volume_name(handle) {
            return Some(handle);
        }
    }
    None
}

pub fn open_fs_handle(fs_handle: &Handle) -> Option<FileSystem> {
    Some(FileSystem::new(
        uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(*fs_handle).ok()?,
    ))
}

fn volume_name_from_root_dir(root_dir: &mut Directory) -> CString16 {
    if let Ok(info_box) = root_dir.get_boxed_info::<FileSystemVolumeLabel>() {
        let info = Box::leak(info_box);
        return info.volume_label().into();
    }
    CString16::try_from("[Volume name error]").unwrap()
}

pub struct EFI {
    pub volume_name: CString16,
    pub file_path: CString16,
}

impl fmt::Display for EFI {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\\{}", self.volume_name, self.file_path)
    }
}

fn get_device_path_string_for_file(fs_handle: &Handle, file_path: &CString16) -> Option<CString16> {
    let mut scoped_prot = uefi::boot::open_protocol_exclusive::<DevicePath>(*fs_handle).ok()?;
    let fs_dpath: &DevicePath = scoped_prot.get_mut()?;

    let mut fs_dpath_string = fs_dpath
        .to_string(DisplayOnly(false), AllowShortcuts(true))
        .ok()?;

    fs_dpath_string.push_str(&CString16::try_from("/").unwrap());
    fs_dpath_string.push_str(file_path);

    Some(fs_dpath_string)
}

pub fn get_device_path_for_file(
    fs_handle: &Handle,
    file_path: &CString16,
) -> Option<Box<DevicePath>> {
    let dpath_string = get_device_path_string_for_file(fs_handle, file_path)?;

    let handle = uefi::boot::get_handle_for_protocol::<DevicePathFromText>().ok()?;
    let binding = uefi::boot::open_protocol_exclusive::<DevicePathFromText>(handle).ok()?;
    let device_path_from_text: &DevicePathFromText = binding.get()?;

    Some(
        device_path_from_text
            .convert_text_to_device_path(dpath_string.as_ref())
            .ok()?
            .to_boxed(),
    )
}

pub fn start_efi(image_handle: &Handle, device_path: &DevicePath) {
    match uefi::boot::load_image(
        *image_handle,
        LoadImageSource::FromDevicePath {
            device_path: device_path,
            boot_policy: BootPolicy::BootSelection,
        },
    ) {
        Ok(loaded_image) => {
            println!("Starting image...\n\n");
            uefi::boot::stall(1_500_000);

            let _ = uefi::boot::start_image(loaded_image);
            println!("The EFI application exited");
        }
        Err(err) => {
            println!("Failed to load EFI image into buffer because of: {}", err);
        }
    }
}

pub struct Drive {
    pub idx: u8,
    pub medium: Medium,
    pub partitions: Vec<Partition>,
}

impl Drive {
    pub fn linux_name(&self) -> CString16 {
        CString16::try_from(format!("/dev/sd{}", ('a' as u8 + self.idx) as char).as_str()).unwrap()
    }
}

impl fmt::Display for Drive {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} size: {}",
            self.linux_name(),
            self.medium.human_readable_size()
        )
    }
}

pub struct Partition {
    pub drive_idx: u8,
    pub idx: u8,
    pub medium: Medium,
    pub format: PartitionFormat,
}

impl Partition {
    pub fn linux_name(&self) -> CString16 {
        CString16::try_from(
            format!("sd{}{}", ('a' as u8 + self.drive_idx) as char, self.idx).as_str(),
        )
        .unwrap()
    }
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} format: {}, size: {}",
            self.linux_name(),
            self.format,
            self.medium.human_readable_size()
        )
    }
}

#[derive(Clone, Copy)]
pub struct Medium {
    pub handle: Handle,
    pub media_id: u32,
    pub size: u64,
}

impl Medium {
    pub fn human_readable_size(&self) -> CString16 {
        const K: u64 = 1024;
        const M: u64 = 1024 * K;
        const G: u64 = 1024 * M;

        CString16::try_from(
            if self.size > 10 * G {
                format!("{}GB", self.size / G)
            } else if self.size > 10 * M {
                format!("{}MB", self.size / M)
            } else if self.size > 10 * K {
                format!("{}KB", self.size / K)
            } else {
                format!("{}B", self.size)
            }
            .as_str(),
        )
        .unwrap()
    }
}

pub enum PartitionFormat {
    Ext4, // ext4
    Fat,  // any of the fat systems supported by UEFI (fat16, fat24, fat32)
    Unknown,
}

impl fmt::Display for PartitionFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Ext4 => write!(f, "EXT4"),
            Self::Fat => write!(f, "FAT"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

pub fn get_drives() -> Vec<Drive> {
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
    idx = 0;
    for &handle in &block_handles {
        if supports_protocol::<PartitionInfo>(handle) {
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
                idx,
                format: determine_partition_format(medium),
                medium,
            };

            parent_drive.partitions.push(part);

            idx += 1;
        }
    }

    drives
}

pub fn find_matching_drive(
    drives: &mut Vec<Drive>,
    partition_handle: Handle,
) -> Option<&mut Drive> {
    for drive in drives {
        if partition_handle_matches_drive_handle(drive.medium.handle, partition_handle) {
            return Some(drive);
        }
    }
    None
}

pub fn partition_handle_matches_drive_handle(
    drive_handle: Handle,
    partition_handle: Handle,
) -> bool {
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

pub fn print_device_path(handle: Handle) {
    let scoped_prot = open_protocol_unsafe::<DevicePath>(handle).unwrap();
    let device_path = scoped_prot.get().unwrap();

    println!(
        "{}",
        device_path
            .to_string(DisplayOnly(false), AllowShortcuts(false))
            .unwrap()
    )
}

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

pub fn supports_protocol<P: ProtocolPointer>(handle: Handle) -> bool {
    boot::test_protocol::<P>(OpenProtocolParams {
        handle,
        agent: image_handle(),
        controller: None,
    })
    .unwrap()
}

pub fn determine_partition_format(medium: Medium) -> PartitionFormat {
    if supports_protocol::<SimpleFileSystem>(medium.handle) {
        return PartitionFormat::Fat;
    }

    match Ext4::load(Box::new(medium)) {
        // Ext4Error::Incompatible here means it is ext4 with some features not supported by the ext4_view crate
        Ok(_) | Err(Ext4Error::Incompatible(_)) => PartitionFormat::Ext4,
        _ => PartitionFormat::Unknown,
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
