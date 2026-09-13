use crate::{Frames, RecordBatch, StorageError};
use itertools::Itertools;
use std::ops::RangeBounds;
use std::ops::{Bound, Range};

pub mod arrow;
pub mod iter;

/// A view into a single entry, which may either be vacant or occupied.
///
/// This `enum` is constructed from the [`entry`] method on [`Txn`].
///
/// [`entry`]: crate::Txn::entry
/// [`Txn`]: crate::Txn::entry
pub enum Entry<'a> {
    Vacant(VacantEntry<'a>),
    Occupied(OccupiedEntry<'a>),
}

impl<'a> Entry<'a> {
    pub(crate) fn new(frames: &'a Frames, name: &str) -> Result<Self, StorageError> {
        let interval = Interval::over(name.to_owned())?;

        let entry = match frames.range(interval.range())?.next() {
            Some(Err(err)) => Err(err)?,
            Some(Ok((str, _))) if str < interval.max() => {
                Entry::Occupied(OccupiedEntry { frames, interval })
            }
            _ => Entry::Vacant(VacantEntry {
                frames,
                name: name.to_owned(),
            }),
        };

        Ok(entry)
    }

    /// Ensures a value is in the entry by inserting the default if empty.
    pub fn or_insert(self, default: RecordBatch) -> Result<OccupiedEntry<'a>, StorageError> {
        match self {
            Entry::Occupied(entry) => Ok(entry),
            Entry::Vacant(entry) => entry.insert_entry(default),
        }
    }

    /// Ensures a value is in the entry by inserting the result of the default function if empty,
    pub fn or_insert_with<F>(self, default: F) -> Result<OccupiedEntry<'a>, StorageError>
    where
        F: FnOnce() -> RecordBatch,
    {
        match self {
            Entry::Occupied(entry) => Ok(entry),
            Entry::Vacant(entry) => entry.insert_entry(default()),
        }
    }

    /// Ensures a value is in the entry by inserting, if empty, the result of the default function.
    pub fn or_insert_with_key<F>(self, default: F) -> Result<OccupiedEntry<'a>, StorageError>
    where
        F: FnOnce(String) -> RecordBatch,
    {
        match self {
            Entry::Occupied(entry) => Ok(entry),
            Entry::Vacant(entry) => {
                let key = entry.key().to_owned();
                entry.insert_entry(default(key))
            }
        }
    }

    /// Returns a reference to this entry’s key.
    pub fn key(&self) -> &str {
        match self {
            Entry::Occupied(entry) => entry.key(),
            Entry::Vacant(entry) => entry.key(),
        }
    }
}

pub struct VacantEntry<'a> {
    frames: &'a Frames,
    name: String,
}

impl<'a> VacantEntry<'a> {
    /// Sets the value of the entry with the `VacantEntr`y`’s key, and returns an `OccupiedEntry`.
    pub fn insert_entry(self, value: RecordBatch) -> Result<OccupiedEntry<'a>, StorageError> {
        let mut occupied = OccupiedEntry {
            frames: self.frames,
            interval: Interval::over(self.name)?,
        };

        occupied.insert_entry(value)?;
        Ok(occupied)
    }

    /// Take ownership of the key.
    pub fn into_key(self) -> String {
        self.name
    }

    /// Gets a reference to the key that would be used when inserting a value through the `VacantEntry`.
    pub fn key(&self) -> &str {
        &self.name
    }
}

pub struct OccupiedEntry<'a> {
    frames: &'a Frames,
    interval: Interval,
}

impl<'a> OccupiedEntry<'a> {
    /// Appends the new `RecordBatch` to the `OccupiedEntry`.
    pub fn insert_entry(
        &mut self,
        value: RecordBatch,
    ) -> Result<&mut OccupiedEntry<'a>, StorageError> {
        let start = match self.range()?.last().transpose()?.map(|(key, _)| key) {
            Some(key) => key,
            None => self.interval.min(),
        };

        let bytes = RecordBatch::compress(&value)?;
        let range = Interval::open(start.to_owned(), self.interval.max().to_owned())?;

        self.frames
            .range(range.range())?
            .replace(Some(bytes.as_ref()))?;

        Ok(self)
    }

    pub fn remove_entry(self) -> Result<VacantEntry<'a>, StorageError> {
        self.frames.range(self.interval.range())?.replace(None)?;
        Ok(VacantEntry {
            frames: self.frames,
            name: self.interval.into_key(),
        })
    }

    /// Gets a reference to the key in the entry.
    pub fn key(&self) -> &str {
        self.interval.as_ref()
    }

    pub(crate) fn range(&self) -> Result<crate::Range<'a>, StorageError> {
        self.frames.range(self.interval.range())
    }
}

/// Represents all of the keys between `"{key}\u{F0000}"` and `"{key}\u{10FFFF}"`.
///
/// It is used internally by `OccupiedEntry` to allow it to store all of `key`’s
/// values in multiple `RecordBatch`s.
struct Interval {
    min: String,
    max: String,
}

/// The entire suffix range is 4-bytes (in utf-8).
///
///   | codepoint | utf-8       |
///   | --------- | ----------- |
///   | F0000     | F0 B1 AA B0 |
///   | 10FFFF    | F4 8F BF BF |
const SUFFIX_LEN: usize = 4;

impl Interval {
    /// Returns a semi-open interval covering the range over `key`
    fn over(mut key: String) -> Result<Self, StorageError> {
        let mut max = key.clone();
        max.push('\u{10FFFF}'); // end of the Private Use Area(s)
        key.push('\u{F0000}'); // beginning of the Private Use Area(s)

        Ok(Self { min: key, max })
    }

    /// Returns an open interval between `min` and `max`
    fn open(mut min: String, max: String) -> Result<Self, StorageError> {
        let mut index = min.pop().expect("key");
        index = char::from_u32(index as u32 + 1).ok_or(StorageError::Full)?;
        min.push(index);

        Ok(Self { min, max })
    }

    fn key_len(&self) -> usize {
        self.min.len() - SUFFIX_LEN
    }

    fn into_key(mut self) -> String {
        self.min.truncate(self.key_len());
        self.min
    }

    fn range(&self) -> Range<&str> {
        Range {
            start: &self.min,
            end: &self.max,
        }
    }

    fn min(&self) -> &str {
        &self.min
    }

    fn max(&self) -> &str {
        &self.max
    }
}

impl RangeBounds<str> for Interval {
    fn start_bound(&self) -> Bound<&str> {
        Bound::Included(self.min())
    }

    fn end_bound(&self) -> Bound<&str> {
        Bound::Excluded(self.max())
    }
}

impl AsRef<str> for Interval {
    fn as_ref(&self) -> &str {
        &self.min[..self.key_len()]
    }
}

impl Frames {
    pub fn all_keys(
        &self,
    ) -> Result<impl Iterator<Item = Result<&str, StorageError>>, StorageError> {
        let interval = Interval::over("".to_owned())?;
        let range = self.range(interval.range())?;

        let iter = range
            .map_ok(|(key, _)| {
                // suffixes are private to `OccupiedEntry`; map them all to the same key
                let len = key.len() - SUFFIX_LEN;
                &key[..len]
            })
            .dedup_by(|lhs, rhs| match (lhs, rhs) {
                (Ok(key1), Ok(key2)) => key1 == key2,
                _ => false,
            });

        Ok(iter)
    }
}
