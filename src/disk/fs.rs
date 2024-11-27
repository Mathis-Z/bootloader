extern crate alloc;

use alloc::vec::Vec;

use ext4_view::{Ext4, Ext4Error};
use uefi::boot::ScopedProtocol;
use uefi::proto::media::file::FileMode;
use uefi::{
    proto::media::{
        file::{FileHandle, FileInfo},
        fs::SimpleFileSystem,
    },
    CString16,
};
use uefi_raw::protocol::file_system::FileAttribute;

// trait to abstract the ext4_view crate and uefi FAT driver into one interface
pub trait Filesystem {
    fn read_file(&mut self, path: CString16) -> Result<Vec<u8>, FileError>;
    fn read_directory(&mut self, path: CString16) -> Result<Directory, FileError>;
}

pub enum FileError {
    NotFound,
    NotAFile,
    NotADirectory,
    Other,
}

pub struct Directory {
    files: Vec<File>,
    full_path: CString16,
}

impl Directory {
    pub fn files(&self) -> &Vec<File> {
        &self.files
    }

    pub fn full_path(&self) -> &CString16 {
        &self.full_path
    }
}

pub struct File {
    name: CString16,
    ftype: FileType,
    size: u64,
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

#[derive(Debug, Copy, Clone)]
pub enum FileType {
    File,
    Directory,
    Other,
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
    fn read_file(&mut self, path: CString16) -> Result<Vec<u8>, FileError> {
        match self.read(ext4_view::Path::new(path.as_bytes())) {
            Ok(data) => Ok(data),
            Err(error) => Err(match error {
                Ext4Error::NotFound => FileError::NotFound,
                Ext4Error::IsADirectory | Ext4Error::IsASpecialFile => FileError::NotAFile,
                _ => FileError::Other,
            }),
        }
    }

    fn read_directory(&mut self, path: CString16) -> Result<Directory, FileError> {
        match self.read_dir(ext4_view::Path::new(path.as_bytes())) {
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

                Ok(Directory {
                    files,
                    full_path: path,
                })
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

        Ok(Directory {
            files,
            full_path: path,
        })
    }
}

fn uefi_get_file_handle(
    fs: &mut SimpleFileSystem,
    path: &CString16,
) -> Result<FileHandle, FileError> {
    let Ok(mut root_directory) = fs.open_volume() else {
        return core::prelude::v1::Err(FileError::Other);
    };

    match uefi::proto::media::file::File::open(
        &mut root_directory,
        path,
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
