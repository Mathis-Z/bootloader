extern crate alloc;
use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

use uefi::{
    data_types::EqStrUntilNul,
    prelude::BootServices,
    print, println,
    proto::media::{
        file::{Directory, File, FileInfo, FileMode, FileSystemInfo, FileSystemVolumeLabel},
        fs::SimpleFileSystem,
        partition::{PartitionInfo, PartitionType},
    },
    table::boot::LoadImageSource,
    CStr16, Char16, Handle,
};
use uefi::{
    proto::media::file::{self, FileAttribute, FileHandle},
    CString16,
};

fn print_mbr_partition_info(partition_info: &PartitionInfo) {
    let mbr_part_entry = partition_info
        .mbr_partition_record()
        .expect("MBR PartitionInfo did not give MbrPartitionrecord :(");
    println!("MBR Partition entry: {:?}", mbr_part_entry);
}

pub fn print_partion_infos(bs: &BootServices) {
    let part_handles = bs
        .find_handles::<PartitionInfo>()
        .expect("Expected to find PartitionInfo handles");
    for handle in part_handles {
        let protocol = bs.open_protocol_exclusive::<PartitionInfo>(handle);

        match protocol {
            Ok(scoped_prot) => match scoped_prot.get() {
                Some(partition_info) => match partition_info.partition_type {
                    PartitionType::GPT => {
                        println!("Found GPT partition");
                    }
                    PartitionType::MBR => {
                        print_mbr_partition_info(partition_info);
                    }
                    _ => {}
                },
                None => {
                    println!("This is unexpected... :/");
                }
            },
            Err(_) => {
                println!("Failed to open exclusive protocol on handle...")
            }
        }
    }
}

pub fn print_all_root_dir_info(bs: &BootServices) {
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");

    for handle in handles {
        println!(
            "Trying to print info about root dir for handle {:?}",
            handle
        );
        if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(handle) {
            if let Some(fs_protocol) = scoped_prot.get_mut() {
                match fs_protocol.open_volume() {
                    Ok(root_directory) => {
                        println!("Got directory info:");
                        print_directory_info(root_directory, true);
                        print!("\n\n");
                    }
                    Err(error) => {
                        println!("Failed to open volume because of: {}", error);
                    }
                }
            } else {
                println!("Failed to resolve ScopedProtocol :(")
            }
        } else {
            println!("Failed to open protocol exclusively :(");
        }
    }
}

pub fn print_directory_info(mut dir: Directory, is_root: bool) {
    if let Ok(boxed_info) = dir.get_boxed_info::<FileInfo>() {
        println!("{:?}\n", boxed_info);
    }
    if is_root {
        if let Ok(boxed_info) = dir.get_boxed_info::<FileSystemInfo>() {
            println!("{:?}\n", boxed_info);
        }
        if let Ok(boxed_info) = dir.get_boxed_info::<FileSystemVolumeLabel>() {
            println!("{:?}\n", boxed_info);
        }
    }
}

pub fn get_volume_names(bs: &BootServices) -> Vec<&CStr16> {
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");
    let mut names = Vec::new();

    for handle in handles {
        get_volume_name(bs, handle);
    }
    return names;
}

pub fn get_volume_name(bs: &BootServices, fs_handle: Handle) -> String {
    if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(fs_handle) {
        if let Some(fs_protocol) = scoped_prot.get_mut() {
            if let Ok(mut root_directory) = fs_protocol.open_volume() {
                if let Ok(info_box) = root_directory.get_boxed_info::<FileSystemVolumeLabel>() {
                    let info = Box::leak(info_box);
                    return info.volume_label().to_string();
                }
            }
        }
    }

    return alloc::format!("Volume {:?}", fs_handle);
}

pub struct EFI {
    pub file_system_handle: Handle,
    pub file_path: CString16,
}

pub fn search_efis(bs: &BootServices) -> Vec<EFI> {
    let mut efis = Vec::new();
    let handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");

    for fs_handle in handles {
        if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(fs_handle) {
            if let Some(fs_protocol) = scoped_prot.get_mut() {
                if let Ok(root_directory) = fs_protocol.open_volume() {
                    if let Some(mut efi_dir) =
                        subdirectory_with_name(root_directory, String::from("EFI"), false)
                    {
                        let efi_dir_file_info = efi_dir.get_boxed_info::<FileInfo>().unwrap();
                        let efi_dir_name = efi_dir_file_info.file_name();

                        for file_path in search_efi_paths_in_directory(&mut efi_dir) {
                            let mut prefixed_path = CString16::new();
                            prefixed_path.push_str(efi_dir_name);
                            prefixed_path.push(Char16::try_from('\\').unwrap());
                            prefixed_path.push_str(&file_path);

                            efis.push(EFI {
                                file_system_handle: fs_handle,
                                file_path: prefixed_path,
                            });
                        }
                    }
                }
            }
        }
    }
    return efis;
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
    mut dir: Directory,
    name: String,
    case_sensitive: bool,
) -> Option<Directory> {
    for file_info in file_infos(&mut dir) {
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

fn open_subdirectory(mut dir: Directory, subname: &CStr16) -> Option<Directory> {
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
                println!("got filesize");
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

fn start_efi(_image_handle: Handle, bs: &BootServices, efi: EFI) {
    if let Ok(scoped_prot) = bs.open_protocol_exclusive::<SimpleFileSystem>(efi.file_system_handle)
    {
        if let Some(fs_protocol) = scoped_prot.get_mut() {
            if let Ok(root_directory) = fs_protocol.open_volume() {
                if let Some(data) = read_file(root_directory, efi.file_path.as_ref()) {
                    println!("EFI image code loaded into buffer!");

                    if let Ok(efi_image_handle) = bs.load_image(
                        _image_handle,
                        LoadImageSource::FromBuffer {
                            buffer: data.as_slice(),
                            file_path: None,
                        },
                    ) {
                        println!("EFI image loaded from buffer...");
                        println!("Starting image now, strap in...");
                        bs.stall(2_000_000);

                        let _ = bs.start_image(efi_image_handle);
                    } else {
                        println!("Failed to load EFI image into buffer")
                    }
                }
            }
        }
    }
}
