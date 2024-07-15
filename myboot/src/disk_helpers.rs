extern crate alloc;

use alloc::{boxed::Box, fmt, format, vec::Vec};

use uefi::CString16;
use uefi::{
    fs::FileSystem,
    prelude::BootServices,
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
    table::boot::LoadImageSource,
    Handle,
};

pub fn get_volume_names(bs: &BootServices) -> Vec<CString16> {
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");
    let mut names = Vec::new();

    for handle in handles {
        names.push(get_volume_name(bs, handle));
    }
    return names;
}

// TODO: Find better fallback name if volume label is empty
pub fn get_volume_name(bs: &BootServices, fs_handle: Handle) -> CString16 {
    if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(fs_handle) {
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

pub fn open_volume_by_name<'a>(bs: &'a BootServices, name: &CString16) -> Option<FileSystem<'a>> {
    if let Some(fs_handle) = fs_handle_by_name(bs, name) {
        return open_fs_handle(bs, &fs_handle);
    }
    None
}

pub fn fs_handle_by_name<'a>(bs: &'a BootServices, name: &CString16) -> Option<Handle> {
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");

    for handle in handles {
        if *name == get_volume_name(bs, handle) {
            return Some(handle);
        }
    }
    None
}

pub fn open_fs_handle<'a>(bs: &'a BootServices, fs_handle: &Handle) -> Option<FileSystem<'a>> {
    Some(FileSystem::new(
        bs.open_protocol_exclusive::<SimpleFileSystem>(*fs_handle)
            .ok()?,
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
    pub file_system_handle: Handle,
    pub volume_name: CString16,
    pub file_path: CString16,
    pub device_path_string: CString16,
    pub device_path: Box<DevicePath>,
}

impl fmt::Display for EFI {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\\{}", self.volume_name, self.file_path)
    }
}

fn get_device_path_string_for_file(
    bs: &BootServices,
    fs_handle: &Handle,
    file_path: &CString16,
) -> Option<CString16> {
    let scoped_prot = bs.open_protocol_exclusive::<DevicePath>(*fs_handle).ok()?;
    let fs_dpath: &DevicePath = scoped_prot.get_mut()?;

    let mut fs_dpath_string = fs_dpath
        .to_string(bs, DisplayOnly(false), AllowShortcuts(true))
        .ok()?;

    fs_dpath_string.push_str(&CString16::try_from("/").unwrap());
    fs_dpath_string.push_str(file_path);

    Some(fs_dpath_string)
}

pub fn get_device_path_for_file(
    bs: &BootServices,
    fs_handle: &Handle,
    file_path: &CString16,
) -> Option<Box<DevicePath>> {
    let dpath_string = get_device_path_string_for_file(bs, fs_handle, file_path)?;

    let handle = bs.get_handle_for_protocol::<DevicePathFromText>().ok()?;
    let binding = bs
        .open_protocol_exclusive::<DevicePathFromText>(handle)
        .ok()?;
    let device_path_from_text: &DevicePathFromText = binding.get()?;

    Some(
        device_path_from_text
            .convert_text_to_device_path(dpath_string.as_ref())
            .ok()?
            .to_boxed(),
    )
}

pub fn start_efi(_image_handle: &Handle, bs: &BootServices, device_path: &DevicePath) {
    match bs.load_image(
        *_image_handle,
        LoadImageSource::FromDevicePath {
            device_path: device_path,
            from_boot_manager: true,
        },
    ) {
        Ok(loaded_image) => {
            println!("Starting image...\n\n");
            bs.stall(1_500_000);

            let _ = bs.start_image(loaded_image);
            println!("The EFI application exited");
        }
        Err(err) => {
            println!("Failed to load EFI image into buffer because of: {}", err);
        }
    }
}
