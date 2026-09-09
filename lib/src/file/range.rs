use crate::file::{Cursor, File};
use crate::{Frames, StorageError};
use lsm1::{
    Seek, lsm_csr_close, lsm_csr_cmp, lsm_csr_key, lsm_csr_next, lsm_csr_open, lsm_csr_prev,
    lsm_csr_seek, lsm_csr_valid, lsm_csr_value, lsm_delete, lsm_delete_range, lsm_insert,
};
use std::marker::PhantomData;
use std::ptr::null_mut;
use std::slice::from_raw_parts;
use std::str::from_utf8_unchecked;

impl Frames {
    pub fn range(&self, range: std::ops::Range<&str>) -> Result<Range<'_>, StorageError> {
        Range::new(self.file, range)
    }
}

/// An iterator over a sub-range of `Entry`s.
///
/// See [`std::collections::btree_map::Range`] for comparison.
pub struct Range<'a> {
    marker: PhantomData<&'a u8>,
    cursor: Cursor,
    file: File,
    end: String,
}

impl<'a> Range<'a> {
    pub(crate) fn new(file: File, range: std::ops::Range<&str>) -> Result<Self, StorageError> {
        let mut cursor = null_mut();

        unsafe {
            lsm_csr_open(file, &mut cursor).ok()?;
            lsm_csr_seek(
                cursor,
                range.start.as_ptr(),
                range.start.len().try_into()?,
                Seek::GE,
            )
            .ok()
            .map_err(|error| {
                let _ = lsm_csr_close(cursor); // close cursor on error
                error
            })?;
        };

        Ok(Self {
            marker: Default::default(),
            cursor,
            file,
            end: range.end.to_owned(),
        })
    }

    /// More akin to [`std::ops::Range`] than [`std::collections::btree_map::Range`]
    pub fn start(&self) -> Result<&str, StorageError> {
        let mut ptr: *const u8 = null_mut();
        let mut len = 0;

        let key = unsafe {
            lsm_csr_key(self.cursor, &mut ptr, &mut len).ok()?;
            from_utf8_unchecked(from_raw_parts(ptr, len as usize))
        };

        Ok(key)
    }

    /// More akin to [`std::ops::Range`] than [`std::collections::btree_map::Range`]
    pub fn end(&self) -> Result<&str, StorageError> {
        let mut cursor = null_mut();
        let mut ptr: *const u8 = null_mut();
        let mut len = 0;

        unsafe {
            let result = (|| {
                lsm_csr_open(self.file, &mut cursor).ok()?;
                lsm_csr_seek(
                    cursor,
                    self.end.as_ptr(),
                    self.end.len().try_into()?,
                    Seek::LE,
                )
                .ok()?;

                lsm_csr_key(self.cursor, &mut ptr, &mut len).ok()?;
                Ok(from_utf8_unchecked(from_raw_parts(ptr, len as usize)))
            })();

            let _ = lsm_csr_close(cursor); // handles nulls properly
            result
        }
    }

    /// # Note
    /// This will **not** be an atomic operation if not performed within a transaction
    pub(crate) fn replace(&self, with: Option<&[u8]>) -> Result<(), StorageError> {
        let start = self.start()?;
        let end = self.end()?;

        unsafe {
            lsm_delete_range(
                self.file,
                start.as_ptr(),
                start.len() as i32,
                end.as_ptr(),
                end.len() as i32,
            )
            .ok()?;

            match with {
                None => lsm_delete(self.file, start.as_ptr(), start.len() as i32).ok()?,
                Some(value) => lsm_insert(
                    self.file,
                    start.as_ptr(),
                    start.len() as i32,
                    value.as_ptr(),
                    value.len().try_into()?,
                )
                .ok()?,
            }
        }

        Ok(())
    }
}

impl<'a> Iterator for Range<'a> {
    type Item = Result<(&'a str, &'a [u8]), StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| unsafe {
            if lsm_csr_valid(self.cursor) == false {
                return Ok(None);
            }

            let mut cmp = 0;
            lsm_csr_cmp(
                self.cursor,
                self.end.as_ptr(),
                self.end.len().try_into()?,
                &mut cmp,
            )
            .ok()?;

            if cmp >= 0 {
                return Ok(None);
            }

            let mut ptr: *const u8 = null_mut();
            let mut len: i32 = 0;

            lsm_csr_key(self.cursor, &mut ptr, &mut len).ok()?;
            let key = str::from_utf8_unchecked(from_raw_parts(ptr, len as usize));

            lsm_csr_value(self.cursor, &mut ptr, &mut len).ok()?;
            let value = from_raw_parts(ptr, len as usize);

            lsm_csr_next(self.cursor).ok()?;
            Ok(Some((key, value)))
        })();

        result.transpose()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        let result = (|| unsafe {
            if lsm_csr_valid(self.cursor) == false {
                return Ok(None);
            }

            let mut ptr = self.end.as_ptr();
            let mut len = self.end.len().try_into()?;

            // Seek::LT does not exist…
            lsm_csr_seek(self.cursor, ptr, len, Seek::LE).ok()?;

            let mut cmp = 0;
            lsm_csr_cmp(self.cursor, ptr, len, &mut cmp).ok()?;

            // …so we back up one (if needed)
            if cmp == 0 {
                lsm_csr_prev(self.cursor).ok()?
            }

            lsm_csr_key(self.cursor, &mut ptr, &mut len).ok()?;
            let key = from_utf8_unchecked(from_raw_parts(ptr, len as usize));

            lsm_csr_value(self.cursor, &mut ptr, &mut len).ok()?;
            let value = from_raw_parts(ptr, len as usize);

            lsm_csr_next(self.cursor).ok()?;
            Ok(Some((key, value)))
        })();

        result.transpose()
    }
}
