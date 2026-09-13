use log::error;
use std::fmt::{Debug, Display, Formatter};

#[derive(Debug)]
#[non_exhaustive]
pub enum StorageError {
    Io(std::io::Error),
    Lsm(lsm1::Error),
    Arrow(arrow::error::ArrowError),
    Limits(std::num::TryFromIntError),
    NulError(std::ffi::NulError),
    Decompression(String),
    Compression(String),
    Empty,
    Full,
}

impl std::error::Error for StorageError {}

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Debug>::fmt(self, f)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error)
    }
}

impl From<std::num::TryFromIntError> for StorageError {
    fn from(error: std::num::TryFromIntError) -> Self {
        StorageError::Limits(error)
    }
}

impl From<std::ffi::c_str::NulError> for StorageError {
    fn from(error: std::ffi::c_str::NulError) -> Self {
        StorageError::NulError(error)
    }
}

impl From<lsm1::Error> for StorageError {
    fn from(error: lsm1::Error) -> Self {
        StorageError::Lsm(error)
    }
}

impl From<arrow::error::ArrowError> for StorageError {
    #[track_caller]
    fn from(error: arrow::error::ArrowError) -> Self {
        match error {
            arrow::error::ArrowError::IoError(string, error) => {
                error!("{}", string);
                StorageError::Io(error)
            }
            _ => StorageError::Arrow(error),
        }
    }
}
