extern crate alloc;

use core::fmt;

#[derive(Debug)]
pub struct SimpleError {
    pub msg: alloc::string::String,
}

impl fmt::Display for SimpleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl core::error::Error for SimpleError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        None
    }
}

impl From<uefi::Error> for SimpleError {
    fn from(error: uefi::Error) -> Self {
        SimpleError {
            msg: alloc::format!("{error}"),
        }
    }
}
