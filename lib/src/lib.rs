#![deny(unsafe_code)]
mod error;
mod file;
mod txn;

pub use error::StorageError;
pub use file::Range;
pub use txn::Txn;

use crate::file::File;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub struct Frames {
    path: PathBuf,
    file: File,
}

impl Frames {
    pub fn path(path: impl AsRef<Path>) -> Result<Frames, StorageError> {
        Self::new(path.as_ref())
    }

    pub fn temporary() -> Result<Frames, StorageError> {
        let temp = NamedTempFile::new()?;
        Self::path(temp.path())
    }

    pub fn file_path(&self) -> Result<String, StorageError> {
        Ok(self.path.to_string_lossy().into_owned())
    }

    pub fn file_size(&self) -> Result<u64, StorageError> {
        Ok(std::fs::metadata(&self.path)?.len())
    }
}

#[derive(Copy, Clone)]
// https://aras-p.info/blog/2021/08/06/EXR-Zstandard-compression/
pub enum Compression {
    /// 1 — 2.463 ratio, 837.4 MB/s writes, 2012.3 MB/s reads
    Fast,
    /// 3 — 2.446 ratio, 745.1 MB/s writes, 1949.8 MB/s reads
    Default,
    /// 14 — 2.517 ratio, 107.1 MB/s writes, 2257.6 MB/s reads
    Good,
    /// 22 — 2.543 ratio, 39.3 MB/s writes, 2014.4 MB/s reads
    Best,
}
