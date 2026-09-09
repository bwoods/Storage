use crate::{Frames, StorageError};
use std::num::NonZeroU32;

pub mod arrow;
pub mod entry;

pub struct Txn<'a> {
    frames: &'a Frames,
    level: NonZeroU32,
}

impl Frames {
    // Note: `&mut self` to prevent a sibling txn() call until this one is drop()’d
    pub fn txn(&mut self) -> Result<Txn<'_>, StorageError> {
        Ok(Txn {
            frames: self,
            level: self.begin(0)?,
        })
    }
}

impl Txn<'_> {
    // Note: `&mut self` to prevent a sibling txn() call until this one is drop()’d
    pub fn txn(&mut self) -> Result<Txn<'_>, StorageError> {
        Ok(Txn {
            frames: self.frames,
            level: self.frames.begin(self.level.get() + 1)?,
        })
    }

    pub fn commit(self) -> Result<(), StorageError> {
        self.frames.commit(self.level.get())
    }

    pub fn rollback(self) -> Result<(), StorageError> {
        self.frames.rollback(self.level.get())
    }
}

impl Drop for Txn<'_> {
    fn drop(&mut self) {
        // This does nothing for already commit()’d or rollback()’d transactions
        self.frames.rollback(self.level.get()).expect("~tx");
    }
}
