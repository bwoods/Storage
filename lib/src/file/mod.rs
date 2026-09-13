//! Low-level file-handling methods
//!
//! All calls into unsafe code are contained in this module.
#![allow(unsafe_code)]

use crate::Frames;
use crate::StorageError;
use lsm1::{
    Config, Info, lsm_begin, lsm_checkpoint, lsm_close, lsm_commit, lsm_config, lsm_cursor, lsm_db,
    lsm_free, lsm_get_env, lsm_info, lsm_new, lsm_open, lsm_rollback, lsm_work,
};
use std::ffi::{CStr, CString};
use std::fs::canonicalize;
use std::num::NonZeroU32;
use std::path::Path;
use std::ptr::null_mut;

mod range;

use crate::entry::Entry;
pub use range::Range;

pub type File = *mut lsm_db;
pub type Cursor = *mut lsm_cursor;

impl Frames {
    pub(crate) fn new(path: &Path) -> Result<Self, StorageError> {
        let path = canonicalize(path)?;
        let cstr = CString::new(path.as_os_str().as_encoded_bytes())?;

        let mut file = null_mut();
        unsafe {
            lsm_new(null_mut(), &mut file).ok()?;

            let off = 0i32;
            lsm_config(file, Config::UseLog, &off).ok()?;
            lsm_config(file, Config::MultipleProcesses, &off).ok()?;

            lsm_open(file, cstr.as_ptr() as *const u8).ok()?;
        }

        Ok(Self { file, path })
    }

    // Note: `&mut self` to prevent a sibling txn() call until this one is drop()’d
    pub fn txn(&mut self) -> Result<Txn<'_>, StorageError> {
        Txn::new(self, 0)
    }

    fn config_value(&self, config: Config, value: Option<u32>) -> Result<u32, StorageError> {
        let mut value = value.unwrap_or_default();
        unsafe { lsm_config(self.file, config, &mut value).ok()? }

        Ok(value)
    }

    pub fn block_size(&self) -> Result<u32, StorageError> {
        self.config_value(Config::BlockSize, None)
            .map(|size| size * 1024) // value is in KiB
    }

    pub fn page_size(&self) -> Result<u32, StorageError> {
        self.config_value(Config::PageSize, None)
    }

    pub fn compact(&mut self) -> Result<bool, StorageError> {
        let mut written = 0i32;

        unsafe {
            lsm_checkpoint(self.file, null_mut()).ok()?;

            // “In order to optimize the database, lsm_work() should be called with the nMerge
            //  argument set to 1 and the third parameter set to a negative value (interpreted as
            // ‘keep working until there is no more work to do’).”
            lsm_work(self.file, 1, -1, &mut written).ok()?;
        }

        Ok(written != 0)
    }

    pub fn info(&mut self) -> Result<String, StorageError> {
        let str = unsafe {
            let mut out = null_mut();
            lsm_info(self.file, Info::DbStructure, &mut out).ok()?;

            if out == null_mut() {
                Err(StorageError::Empty)?
            } else {
                let str = CStr::from_ptr(out).to_string_lossy().to_string();
                lsm_free(lsm_get_env(self.file), out.cast());
                str
            }
        };

        Ok(str)
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        // The borrow checker should prevent the existence of any lingering
        // transactions or cursors that would cause this call to fail.
        unsafe { lsm_close(self.file) }.ok().expect("~db")
    }
}

pub struct Txn<'a> {
    frames: &'a Frames,
    level: NonZeroU32,
}

impl<'a> Txn<'a> {
    /// Gets the entry for in-place manipulation.
    pub fn entry(&mut self, name: &str) -> Result<Entry<'_>, StorageError> {
        Entry::new(self.frames, name)
    }

    pub fn new(frames: &'a Frames, txn: u32) -> Result<Self, StorageError> {
        let level = unsafe { NonZeroU32::new_unchecked(txn + 1) };
        unsafe { lsm_begin(frames.file, level.get().try_into()?).ok()? }

        Ok(Txn { frames, level })
    }

    // Note: `&mut self` to prevent a sibling txn() call until this one is drop()’d
    pub fn txn(&mut self) -> Result<Txn<'_>, StorageError> {
        Self::new(self.frames, self.level.get())
    }

    pub fn commit(self) -> Result<(), StorageError> {
        // SAFETY: level.get() already checked in new()
        unsafe { lsm_commit(self.frames.file, self.level.get() as i32).ok()? }
        Ok(())
    }

    pub fn rollback(self) -> Result<(), StorageError> {
        drop(self);
        Ok(())
    }
}

impl Drop for Txn<'_> {
    // This does nothing for already commit()’d or rollback()’d transactions
    fn drop(&mut self) {
        // SAFETY: level.get() already checked in new()
        unsafe { lsm_rollback(self.frames.file, self.level.get() as i32) }
            .ok()
            .expect("~tx")
    }
}
