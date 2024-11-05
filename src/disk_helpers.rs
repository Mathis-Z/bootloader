extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec};

use uefi::boot::LoadImageSource;
use uefi::fs::FileSystem;
use uefi::proto::BootPolicy;
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
