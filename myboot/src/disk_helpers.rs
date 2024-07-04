extern crate alloc;

use alloc::{
    boxed::Box,
    fmt,
    string::{String, ToString},
    vec::Vec,
};

use uefi::{
    data_types::EqStrUntilNul,
    prelude::BootServices,
    println,
    proto::{
        device_path::{
            text::{AllowShortcuts, DevicePathFromText, DevicePathToText, DisplayOnly},
            DevicePath,
        },
        media::{
            file::{Directory, File, FileInfo, FileMode, FileSystemVolumeLabel},
            fs::SimpleFileSystem,
        },
    },
    table::boot::LoadImageSource,
    CStr16, Char16, Handle,
};
use uefi::{
    proto::media::file::{FileAttribute, FileHandle},
    CString16,
};

pub fn get_volume_names(bs: &BootServices) -> Vec<&CStr16> {
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");
    let names = Vec::new();

    for handle in handles {
        get_volume_name(bs, handle);
    }
    return names;
}

pub fn get_volume_name(bs: &BootServices, fs_handle: Handle) -> CString16 {
    if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(fs_handle) {
        if let Some(fs_protocol) = scoped_prot.get_mut() {
            if let Ok(mut root_directory) = fs_protocol.open_volume() {
                return volume_name_from_root_dir(&mut root_directory);
            }
        }
    }

    CString16::try_from("[Volume name error]").unwrap()
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

pub fn search_efis(bs: &BootServices) -> Vec<EFI> {
    let mut efis = Vec::new();
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");

    for fs_handle in handles {
        if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(fs_handle) {
            if let Some(fs_protocol) = scoped_prot.get_mut() {
                if let Ok(mut root_directory) = fs_protocol.open_volume() {
                    if let Some(mut efi_dir) =
                        subdirectory_with_name(&mut root_directory, String::from("EFI"), false)
                    {
                        let efi_dir_file_info = efi_dir.get_boxed_info::<FileInfo>().unwrap();
                        let efi_dir_name = efi_dir_file_info.file_name();

                        for file_path in search_efi_paths_in_directory(&mut efi_dir) {
                            let mut prefixed_path = CString16::new();
                            prefixed_path.push_str(efi_dir_name);
                            prefixed_path.push(Char16::try_from('\\').unwrap());
                            prefixed_path.push_str(&file_path);

                            if let Some(dpath) =
                                get_device_path_for_file(bs, &fs_handle, &prefixed_path)
                            {
                                efis.push(EFI {
                                    file_system_handle: fs_handle,
                                    volume_name: volume_name_from_root_dir(&mut root_directory),
                                    file_path: prefixed_path.clone(),
                                    device_path_string: get_device_path_string_for_file(
                                        bs,
                                        &fs_handle,
                                        &prefixed_path.clone(),
                                    )
                                    .expect("failed to get devicepath"),
                                    device_path: dpath,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    return efis;
}

fn device_path_to_string(bs: &BootServices, device_path: &DevicePath) -> CString16 {
    if let Ok(handle) = bs.get_handle_for_protocol::<DevicePathToText>() {
        if let Ok(binding) = bs.open_protocol_exclusive::<DevicePathToText>(handle) {
            if let Some(device_path_to_text) = binding.get() {
                if let Ok(pool_string) = device_path_to_text.convert_device_path_to_text(
                    bs,
                    device_path,
                    DisplayOnly(true),
                    AllowShortcuts(true),
                ) {
                    let mut string = CString16::new();
                    for char in pool_string.iter() {
                        string.push(*char);
                    }
                    return string;
                }
            }
        }
    }

    CString16::try_from("[DevicePath]").unwrap()
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

    fs_dpath_string.push_str(&CString16::try_from("/\\").unwrap());
    fs_dpath_string.push_str(file_path);

    Some(fs_dpath_string)
}

fn get_device_path_for_file(
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

fn duplicate_backticks_in_path(path: &CString16) -> CString16 {
    let mut new_string = CString16::new();

    for char in path.iter() {
        if *char == Char16::try_from('\\').unwrap() {
            new_string.push(*char);
        }
        new_string.push(*char);
    }

    new_string
}

fn search_efi_paths_in_directory(dir: &mut Directory) -> Vec<CString16> {
    let mut efis = Vec::new();

    let files = file_infos(dir);

    for boxed_file_info in files {
        let file_info = &*boxed_file_info;

        if file_info.is_directory()
            && !file_info.file_name().eq_str_until_nul(".")
            && !file_info.file_name().eq_str_until_nul("..")
        {
            if let Ok(subdir_handle) = dir.open(
                file_info.file_name(),
                FileMode::Read,
                FileAttribute::DIRECTORY,
            ) {
                let mut subdir = subdir_handle
                    .into_directory()
                    .expect("Directory is not a directory?!");

                let sub_efis = &mut search_efi_paths_in_directory(&mut subdir);

                for sub_efi in sub_efis {
                    let mut prefixed_path = CString16::new();
                    prefixed_path.push_str(file_info.file_name());
                    prefixed_path.push(Char16::try_from('\\').unwrap());
                    prefixed_path.push_str(sub_efi);
                    efis.push(prefixed_path);
                }
            }
        } else {
            let filename = file_info.file_name();
            let len = filename.num_chars();
            if len > 4 {
                let end_slice = &filename.as_slice_with_nul()[len - 4..];

                if let Ok(end) = CStr16::from_char16_with_nul(end_slice) {
                    if end.eq_str_until_nul(".efi") || end.eq_str_until_nul(".EFI") {
                        efis.push(filename.into());
                    }
                }
            }
        }
    }

    return efis;
}

fn file_infos(dir: &mut Directory) -> Vec<Box<FileInfo>> {
    let mut subs = Vec::new();

    loop {
        if let Some(boxed_info) = dir.read_entry_boxed().unwrap_or(None) {
            subs.push(boxed_info);
        } else {
            break;
        }
    }
    return subs;
}

fn subdirectory_with_name(
    dir: &mut Directory,
    name: String,
    case_sensitive: bool,
) -> Option<Directory> {
    for file_info in file_infos(dir) {
        let filename_cstr = file_info.file_name();
        let filename = filename_cstr.to_string();

        if (case_sensitive && filename == name)
            || (!case_sensitive && filename.to_lowercase() == name.to_lowercase())
        {
            return open_subdirectory(dir, filename_cstr);
        }
    }
    return None;
}

fn open_subdirectory(dir: &mut Directory, subname: &CStr16) -> Option<Directory> {
    if let Ok(file_handle) = dir.open(subname, FileMode::Read, FileAttribute::DIRECTORY) {
        if let Some(subdir) = file_handle.into_directory() {
            return Some(subdir);
        }
    }
    return None;
}

pub(crate) fn read_file(mut dir: Directory, filename: &CStr16) -> Option<Vec<u8>> {
    match dir.open(filename, FileMode::Read, FileAttribute::VALID_ATTR) {
        Ok(mut file_handle) => {
            if let Some(filesize) = file_handle_get_file_size(&mut file_handle) {
                if let Some(mut file) = file_handle.into_regular_file() {
                    let mut data = Vec::new();
                    data.resize(filesize.try_into().unwrap(), 0);

                    if let Ok(bytes_read) = file.read(data.as_mut_slice()) {
                        data.resize(bytes_read, 0);
                        return Some(data);
                    }
                }
            }
        }
        Err(error) => println!("Error: {:?}", error),
    }
    None
}

fn file_handle_get_file_size(handle: &mut FileHandle) -> Option<u64> {
    match handle.get_boxed_info::<FileInfo>() {
        Ok(file_info) => Some(file_info.file_size()),
        Err(_) => None,
    }
}

// splits \path\to\somewhere into ["path", "to", "somewhere"]
fn split_path(path: CString16) -> Vec<CString16> {
    let mut parts = Vec::new();
    let mut str = CString16::new();
    let separator = Char16::try_from('\\').unwrap();

    for char in path.iter() {
        if *char == separator && !str.is_empty() {
            parts.push(str);
            str = CString16::new();
        } else {
            str.push(*char);
        }
    }

    parts
}

pub fn start_efi(_image_handle: &Handle, bs: &BootServices, efi: &EFI) {
    if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(efi.file_system_handle)
    {
        if let Some(fs_protocol) = scoped_prot.get_mut() {
            if let Ok(root_directory) = fs_protocol.open_volume() {
                if let Some(data) = read_file(root_directory, efi.file_path.as_ref()) {
                    if let Ok(efi_image_handle) = bs.load_image(
                        *_image_handle,
                        LoadImageSource::FromBuffer {
                            buffer: data.as_slice(),
                            file_path: None,
                        },
                    ) {
                        println!("Starting image...\n\n");
                        bs.stall(1_500_000);

                        let _ = bs.start_image(efi_image_handle);

                        println!("The EFI application exited");
                    } else {
                        println!("Failed to load EFI image into buffer")
                    }
                }
            }
        }
    }
}

pub fn start_efi2(_image_handle: &Handle, bs: &BootServices, efi: &EFI) {
    match bs.load_image(
        *_image_handle,
        LoadImageSource::FromDevicePath {
            device_path: &efi.device_path,
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
