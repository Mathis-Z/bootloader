extern crate alloc;

use core::fmt::Display;

use alloc::vec::Vec;

use ext4_view::{Ext4, Ext4Error};
use uefi::boot::ScopedProtocol;
use uefi::proto::media::file::FileMode;
use uefi::Char16;
use uefi::{
    proto::media::{
        file::{FileHandle, FileInfo},
        fs::SimpleFileSystem,
    },
    CString16,
};
use uefi_raw::protocol::file_system::FileAttribute;

use crate::disk::human_readable_size;
use crate::simple_error::SimpleResult;

use super::simple_error;

// trait to abstract the ext4_view crate and uefi FAT driver into one interface
pub trait Filesystem {
    fn read_file(&mut self, path: CString16) -> Result<Vec<u8>, FileError>;
    fn read_directory(&mut self, path: CString16) -> Result<Directory, FileError>;
    fn format(&self) -> FsType;
}

#[derive(Copy, Clone)]
pub enum FsType {
    Ext4,
    Fat,
}

impl core::fmt::Display for FsType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ext4 => "EXT4",
                Self::Fat => "FAT",
            }
        )
    }
}

#[derive(Debug, Copy, Clone)]
pub enum FileError {
    NotFound,
    NotAFile,
    NotADirectory,
    Other,
}

pub struct Directory {
    files: Vec<File>,
}

pub struct File {
    name: CString16,
    ftype: FileType,
    size: u64,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FileType {
    File,
    Directory,
    Other,
}

// an absolute path beginning with the partition name
#[derive(Debug, Clone)]
pub struct FsPath {
    pub components: Vec<CString16>,
}

impl FsPath {
    pub fn new() -> FsPath {
        FsPath {
            components: Vec::new(),
        }
    }

    fn parse_components(str: &CString16) -> (Vec<CString16>, bool) {
        let separator: Char16 = Char16::try_from('/').unwrap();

        let mut components = Vec::new();
        let mut component = CString16::new();

        if str.is_empty() {
            return (Vec::new(), false);
        }

        let is_absolute = str.as_slice()[0] == separator;

        for &char in str.iter() {
            if char == separator {
                if !component.is_empty() {
                    components.push(component);
                    component = CString16::new();
                }
            } else {
                component.push(char);
            }
        }

        if !component.is_empty() {
            components.push(component);
        }

        (components, is_absolute)
    }

    pub fn parse<S: alloc::string::ToString>(string_like: &S) -> SimpleResult<FsPath> {
        let string = alloc::string::ToString::to_string(string_like);
        let cstring = CString16::try_from(string.as_str())
            .or_else(|_| simple_error!("'{string}' contained invalid characters"))?;

        let (components, is_absolute) = Self::parse_components(&cstring);

        if is_absolute {
            Ok(FsPath { components })
        } else {
            simple_error!("Path is not absolute!")
        }
    }

    fn merge_dots(&mut self) {
        let dot = &CString16::try_from(".").unwrap();
        let double_dot = &CString16::try_from("..").unwrap();

        let mut new_components: Vec<CString16> = Vec::new();

        for component in &self.components {
            if component == dot {
                continue; // remove all single dots
            } else if component == double_dot {
                if let Some(prev_component) = new_components.last() {
                    if prev_component == double_dot {
                        new_components.push(double_dot.clone()); // cannot merge ".." with "..", just append
                    } else {
                        new_components.pop(); // merge ".." with previous component by popping it
                    }
                } else {
                    new_components.push(double_dot.clone());
                }
            } else {
                new_components.push(component.clone());
            }
        }

        self.components = new_components;
        if self.components.first() == Some(double_dot) {
            self.components = Vec::new(); // cannot go higher than root
        }
    }

    pub fn push(&mut self, other: &CString16) {
        if other.is_empty() {
            return;
        }

        let (mut components, is_absolute) = Self::parse_components(other);

        if is_absolute {
            self.components = components;
        } else {
            self.components.append(&mut components);
            self.merge_dots();
        }
    }

    pub fn to_string(&self, skip_partition_name: bool) -> CString16 {
        if self.components.is_empty() || (skip_partition_name && self.components.len() <= 1) {
            return CString16::try_from("/").unwrap();
        }

        let separator: Char16 = Char16::try_from('/').unwrap();
        let mut out = CString16::new();
        let start_from = if skip_partition_name { 1 } else { 0 };

        for component in &self.components[start_from..] {
            out.push(separator);
            out.push_str(component);
        }

        out
    }

    pub fn path_on_partition(&self) -> CString16 {
        self.to_string(true)
    }
}

impl Display for FsPath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.to_string(false))
    }
}

impl From<FsPath> for CString16 {
    fn from(value: FsPath) -> Self {
        value.to_string(false)
    }
}

impl From<&FsPath> for CString16 {
    fn from(value: &FsPath) -> Self {
        value.to_string(false)
    }
}

impl File {
    pub fn name(&self) -> &CString16 {
        &self.name
    }

    pub fn file_type(&self) -> &FileType {
        &self.ftype
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn is_regular_file(&self) -> bool {
        self.ftype == FileType::File
    }
}

impl Display for File {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self.file_type() {
                FileType::File => "-",
                FileType::Directory => "d",
                FileType::Other => "?",
            }
        )?;

        write!(f, " {} {}", human_readable_size(self.size), self.name())?;
        Ok(())
    }
}

impl TryFrom<ext4_view::DirEntry> for File {
    type Error = anyhow::Error;

    fn try_from(dir_entry: ext4_view::DirEntry) -> Result<Self, Self::Error> {
        let Ok(name) = CString16::try_from(dir_entry.file_name().as_str()?) else {
            anyhow::bail!("Filename to CString16 conversion failed");
        };
        let ftype = dir_entry.file_type()?.into();
        let size = dir_entry.metadata()?.len();

        Ok(File { name, ftype, size })
    }
}

impl From<&FileInfo> for File {
    fn from(file_info: &FileInfo) -> Self {
        let name = file_info.file_name().into();

        let ftype = if file_info.is_directory() {
            FileType::Directory
        } else if file_info.is_regular_file() {
            FileType::File
        } else {
            FileType::Other
        };

        let size = file_info.file_size();

        File { name, ftype, size }
    }
}

impl Directory {
    pub fn empty() -> Self {
        Directory { files: Vec::new() }
    }

    pub fn files(&self) -> &Vec<File> {
        &self.files
    }
}

impl From<ext4_view::FileType> for FileType {
    fn from(ftype: ext4_view::FileType) -> Self {
        match ftype {
            ext4_view::FileType::Regular => Self::File,
            ext4_view::FileType::Directory => Self::Directory,
            _ => Self::Other,
        }
    }
}

impl Filesystem for Ext4 {
    fn format(&self) -> FsType {
        FsType::Ext4
    }

    fn read_file(&mut self, path: CString16) -> Result<Vec<u8>, FileError> {
        let str = alloc::string::String::from_utf16_lossy(path.to_u16_slice());
        let p = ext4_view::Path::new(&str);

        match self.read(p) {
            Ok(data) => Ok(data),
            Err(error) => Err(match error {
                Ext4Error::NotFound => FileError::NotFound,
                Ext4Error::IsADirectory | Ext4Error::IsASpecialFile => FileError::NotAFile,
                _ => FileError::Other,
            }),
        }
    }

    fn read_directory(&mut self, path: CString16) -> Result<Directory, FileError> {
        let str = alloc::string::String::from_utf16_lossy(path.to_u16_slice());
        let p = ext4_view::Path::new(&str);

        match self.read_dir(p) {
            Ok(dir) => {
                let mut files: Vec<File> = Vec::new();

                for dir_entry in dir.into_iter() {
                    let Ok(dir_entry) = dir_entry else {
                        continue;
                    };

                    let Ok(file) = dir_entry.try_into() else {
                        continue;
                    };

                    files.push(file);
                }

                Ok(Directory { files })
            }
            Err(error) => Err(match error {
                Ext4Error::NotFound => FileError::NotFound,
                Ext4Error::IsADirectory | Ext4Error::IsASpecialFile => FileError::NotAFile,
                _ => FileError::Other,
            }),
        }
    }
}

// implementation for uefi FAT driver
impl Filesystem for ScopedProtocol<SimpleFileSystem> {
    fn format(&self) -> FsType {
        FsType::Fat
    }

    fn read_file(&mut self, path: CString16) -> Result<Vec<u8>, FileError> {
        let file_handle = uefi_get_file_handle(self, &path)?;
        let Some(mut file) = file_handle.into_regular_file() else {
            return Err(FileError::NotAFile);
        };

        let mut data = Vec::with_capacity(4096);
        let mut total_bytes_read = 0;

        loop {
            let old = data.len();
            data.resize(old * 2, 0);
            let new_buf = &mut data[old..];

            let Ok(bytes_read) = file.read(new_buf) else {
                return Err(FileError::Other);
            };
            total_bytes_read += bytes_read;

            if bytes_read < new_buf.len() {
                break; // assuming a not completely filled buffer means we reached EOF
            }
        }
        data.resize(total_bytes_read, 0);

        Ok(data)
    }

    fn read_directory(&mut self, path: CString16) -> Result<Directory, FileError> {
        let file_handle = uefi_get_file_handle(self, &path)?;
        let Some(mut dir) = file_handle.into_directory() else {
            return Err(FileError::NotADirectory);
        };

        let mut files = Vec::new();

        loop {
            let Ok(file_info) = dir.read_entry_boxed() else {
                return Err(FileError::Other);
            };

            if let Some(file_info) = file_info {
                files.push(file_info.as_ref().into());
            } else {
                break;
            }
        }

        Ok(Directory { files })
    }
}

fn uefi_get_file_handle(
    fs: &mut SimpleFileSystem,
    path: &CString16,
) -> Result<FileHandle, FileError> {
    let Ok(mut root_directory) = fs.open_volume() else {
        return core::prelude::v1::Err(FileError::Other);
    };

    let mut uefi_path = uefi::fs::PathBuf::new();
    uefi_path.push(path.as_ref());

    match uefi::proto::media::file::File::open(
        &mut root_directory,
        uefi_path.to_cstr16(),
        FileMode::Read,
        FileAttribute::empty(),
    ) {
        Ok(file_handle) => Ok(file_handle),
        Err(error) => Err(match error.status() {
            uefi::Status::NOT_FOUND => FileError::NotFound,
            _ => FileError::Other,
        }),
    }
}
