use core::ops::Deref;

extern crate alloc;
use alloc::vec::Vec;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::{
    prelude::BootServices, print, println, proto::media::partition::PartitionInfo,
    table::boot::SearchType,
};
use uefi::{Handle, Identify};
use uefi_raw::protocol::{
    block::BlockIoProtocol,
    disk::{DiskIo2Protocol, DiskIoProtocol},
    file_system::SimpleFileSystemProtocol,
};
use uefi_raw::Guid;

pub fn print_handles(bs: &BootServices, search_type: SearchType) {
    let handle_buffer = bs
        .locate_handle_buffer(search_type)
        .expect("Failed to enumerate handles!");
    for handle in handle_buffer.deref() {
        print!("Handle {:?} supports protocols: ", handle);

        match bs.protocols_per_handle(*handle) {
            Ok(protocols) => {
                for guid in protocols.to_vec() {
                    if let Some(str) = guid_to_protocol(&guid) {
                        print!("{}, ", str);
                    }
                }
                println!();
            }
            Err(error) => {
                println!("(Error listing protocols: {}", error);
            }
        }
    }
}

fn guid_to_protocol(guid: &Guid) -> Option<&str> {
    match *guid {
        BlockIoProtocol::GUID => Some("BlockIo"),
        DiskIoProtocol::GUID => Some("DiskIo"),
        DiskIo2Protocol::GUID => Some("DiskIo2"),
        SimpleFileSystemProtocol::GUID => Some("SimpleFileSystem"),
        PartitionInfo::GUID => Some("PartitionInfo"),

        // TODO: etc.
        _ => None,
    }
}

pub fn list_efi_partition_handles(bs: &BootServices) -> Vec<Handle> {
    let efi_part_handles = Vec::new();

    let fs_handles = bs
        .find_handles::<SimpleFileSystem>()
        .expect("Failed to get FS handles!");

    for _handle in fs_handles {}

    return efi_part_handles;
}
