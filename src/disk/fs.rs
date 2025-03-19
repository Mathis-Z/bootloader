/*
We use the UEFI FAT file API to read FAT filesystems and the ext4_view crate to read ext4 filesystems.
To unify the access to both filesystems, we define a trait Filesystem that abstracts the differences between the two filesystems.
For this new API we also define File, Directory and FileError types.
We also define the FsPath struct to represent an absolute path on a filesystem. This path includes the partition name as if all
partitions were "mounted" at the root with their names as directories.
*/

extern crate alloc;

use core::fmt::Display;

use alloc::{vec::Vec, string::{String, ToString}};

use ext4_view::{Ext4, Ext4Error};
use uefi::{boot::ScopedProtocol, data_types::FromStrError};
use uefi::proto::media::file::FileMode;
use uefi::{
    proto::media::{
        file::{FileHandle, FileInfo},
        fs::SimpleFileSystem,
    },
    CString16,
};
use uefi_raw::protocol::file_system::FileAttribute;

use crate::{disk::human_readable_size, simple_error::SimpleError};
use crate::simple_error::SimpleResult;

use super::simple_error;

// trait to abstract the ext4_view crate and uefi FAT driver into one interface
pub trait Filesystem {
    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FileError>;
    fn read_directory(&mut self, path: &str) -> Result<Directory, FileError>;
    fn format(&self) -> FsType;
}

#[derive(Copy, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub enum FileError {
    NotFound,
    NotAFile,
    NotADirectory,
    Other,
}

impl From<uefi::Error> for FileError {
    fn from(error: uefi::Error) -> Self {
        match error.status() {
            uefi::Status::NOT_FOUND => FileError::NotFound,
            _ => FileError::Other,
        }
    }
}

pub struct Directory {
    files: Vec<File>,
}

pub struct File {
    name: String,
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
    pub components: Vec<String>,
}

impl FsPath {
    pub fn new() -> FsPath {
        FsPath {
            components: Vec::new(),
        }
    }

    fn parse_components(str: &str) -> (Vec<String>, bool) {
        const SEPARATOR: char = '/';

        let mut components = Vec::new();
        let mut component = String::new();

        let is_absolute = str.starts_with(SEPARATOR);

        for char in str.chars() {
            if char == SEPARATOR {
                if !component.is_empty() {
                    components.push(component);
                    component = String::new();
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

    pub fn parse<S: AsRef<str>>(string_like: S) -> SimpleResult<FsPath> {
        let (components, is_absolute) = Self::parse_components(string_like.as_ref());

        if is_absolute {
            Ok(FsPath { components })
        } else {
            simple_error!("Path is not absolute!")
        }
    }

    fn merge_dots(&mut self) {
        const DOT: &str = ".";
        const DOUBLE_DOT: &str = "..";

        let mut new_components: Vec<String> = Vec::new();

        for component in &self.components {
            if component == DOT {
                continue; // remove all single dots
            } else if component == DOUBLE_DOT {
                if let Some(prev_component) = new_components.last() {
                    if prev_component == DOUBLE_DOT {
                        new_components.push(DOUBLE_DOT.to_string()); // cannot merge ".." with "..", just append
                    } else {
                        new_components.pop(); // merge ".." with previous component by popping it
                    }
                } else {
                    new_components.push(DOUBLE_DOT.to_string());
                }
            } else {
                new_components.push(component.clone());
            }
        }

        self.components = new_components;
        if self.components.first() == Some(&DOUBLE_DOT.to_string()) {
            self.components = Vec::new(); // cannot go higher than root
        }
    }

    pub fn push<S: AsRef<str>>(&mut self, other: S) -> &mut Self {
        if other.as_ref().is_empty() {
            return self;
        }

        let (mut components, is_absolute) = Self::parse_components(other.as_ref());

        if is_absolute {
            self.components = components;
        } else {
            self.components.append(&mut components);
            self.merge_dots();
        }

        self
    }

    fn _to_string(&self, with_partition_name: bool, separator: &str) -> String {
        if self.components.is_empty() || (!with_partition_name && self.components.len() == 1) {
            return "/".to_string();
        }

        let mut out = String::new();
        let start_from = if with_partition_name { 0 } else { 1 };

        for component in &self.components[start_from..] {
            out.push_str(separator);
            out.push_str(component);
        }

        out
    }

    pub fn path_on_partition(&self) -> String {
        self._to_string(false, "/")
    }

    pub fn to_uefi_string(&self, with_partition_name: bool) -> Result<CString16, FromStrError> {
        CString16::try_from(&self._to_string(with_partition_name, "\\") as &str)
    }
}

impl Display for FsPath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self._to_string(true, "/"))
    }
}

impl From<FsPath> for String {
    fn from(path: FsPath) -> Self {
        path._to_string(true, "/")
    }
}

impl From<&FsPath> for String {
    fn from(path: &FsPath) -> Self {
        path._to_string(true, "/")
    }
}

impl File {
    pub fn name(&self) -> &str {
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
    type Error = SimpleError;

    fn try_from(dir_entry: ext4_view::DirEntry) -> Result<Self, Self::Error> {
        let Ok(name) = dir_entry.file_name().as_str() else {
            return simple_error!("Could not convert filename to string!")
        };
        let ftype = dir_entry.file_type()?.into();
        let size = dir_entry.metadata()?.len();

        Ok(File { name: name.to_string(), ftype, size })
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

// implementation for ext4_view crate
impl Filesystem for Ext4 {
    fn format(&self) -> FsType {
        FsType::Ext4
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FileError> {
        let p = ext4_view::Path::new(path);

        match self.read(p) {
            Ok(data) => Ok(data),
            Err(error) => Err(match error {
                Ext4Error::NotFound => FileError::NotFound,
                Ext4Error::IsADirectory | Ext4Error::IsASpecialFile => FileError::NotAFile,
                _ => FileError::Other,
            }),
        }
    }

    fn read_directory(&mut self, path: &str) -> Result<Directory, FileError> {
        let p = ext4_view::Path::new(path);

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

// implementation for UEFI FAT API
impl Filesystem for ScopedProtocol<SimpleFileSystem> {
    fn format(&self) -> FsType {
        FsType::Fat
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FileError> {
        let file_handle = uefi_get_file_handle(self, path)?;
        let Some(mut file) = file_handle.into_regular_file() else {
            return Err(FileError::NotAFile);
        };

        let mut data = Vec::with_capacity(4096);
        let mut total_bytes_read = 0;

        loop {
            let old = data.len();
            data.resize(old * 2 + 1, 0);
            let new_buf = &mut data[old..];

            let bytes_read = file.read(new_buf)?;
            
            total_bytes_read += bytes_read;

            if bytes_read < new_buf.len() {
                break; // assuming a not completely filled buffer means we reached EOF
            }
        }
        data.resize(total_bytes_read, 0);

        Ok(data)
    }

    fn read_directory(&mut self, path: &str) -> Result<Directory, FileError> {
        let file_handle = uefi_get_file_handle(self, path)?;
        let Some(mut dir) = file_handle.into_directory() else {
            return Err(FileError::NotADirectory);
        };

        let mut files = Vec::new();

        loop {
            let file_info = dir.read_entry_boxed()?;

            if let Some(file_info) = file_info {
                files.push(file_info.as_ref().into());
            } else {
                break;
            }
        }

        Ok(Directory { files })
    }
}

// helper function for impl Filesystem for ScopedProtocol<SimpleFileSystem>
fn uefi_get_file_handle<S: AsRef<str>>(
    fs: &mut SimpleFileSystem,
    path: S,
) -> Result<FileHandle, FileError> {
    let mut root_directory = match fs.open_volume() {
        Ok(root_directory) => root_directory,
        Err(_) => return Err(FileError::Other),
    };

    let Ok(cstring_path) = CString16::try_from(path.as_ref()) else {
        return Err(FileError::Other);
    };

    let mut uefi_path = uefi::fs::PathBuf::new();
    uefi_path.push(cstring_path.as_ref());

    Ok(uefi::proto::media::file::File::open(
        &mut root_directory,
        uefi_path.to_cstr16(),
        FileMode::Read,
        FileAttribute::empty(),
    )?)
}
