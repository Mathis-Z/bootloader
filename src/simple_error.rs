extern crate alloc;

use core::fmt;

use ext4_view::Ext4Error;

macro_rules! simple_error {
    ($($args:tt)*) => {
        Err(crate::simple_error::SimpleError {
            msg: alloc::format!($($args)*),
        })
    };
}
pub(crate) use simple_error;

#[derive(Debug)]
pub struct SimpleError {
    pub msg: alloc::string::String,
}

pub type SimpleResult<T> = Result<T, SimpleError>;

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

impl From<Ext4Error> for SimpleError {
    fn from(error: Ext4Error) -> Self {
        SimpleError {
            msg: alloc::format!("{error}"),
        }
    }
}

impl From<&'static str> for SimpleError {
    fn from(error: &'static str) -> Self {
        SimpleError {
            msg: alloc::format!("{error}"),
        }
    }
}
