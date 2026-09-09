use crate::txn::{Txn, arrow::RecordBatch};
use crate::{Frames, StorageError};

impl Txn<'_> {
    pub fn entry(&mut self, name: &str) -> Result<Entry<'_>, StorageError> {
        let range = KeyRange::named(name);

        let entry = match self.frames.range(range.full())?.next() {
            Some(Err(err)) => Err(err)?,
            Some(Ok((str, _))) if str == range.min() => {
                Entry::Occupied(OccupiedEntry::with_range(self.frames, range))
            }
            _ => Entry::Vacant(VacantEntry::new(self.frames, name)),
        };

        Ok(entry)
    }
}

pub enum Entry<'a> {
    Vacant(VacantEntry<'a>),
    Occupied(OccupiedEntry<'a>),
}

impl<'a> Entry<'a> {
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
    fn new(frames: &'a Frames, name: &str) -> Self {
        VacantEntry {
            frames,
            name: name.to_owned(),
        }
    }

    pub fn insert_entry(self, value: RecordBatch) -> Result<OccupiedEntry<'a>, StorageError> {
        let mut occupied = OccupiedEntry::new(self.frames, &self.name);
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
    range: KeyRange,
}

impl<'a> OccupiedEntry<'a> {
    fn new(frames: &'a Frames, name: &str) -> Self {
        Self::with_range(frames, KeyRange::named(name))
    }

    fn with_range(frames: &'a Frames, range: KeyRange) -> Self {
        OccupiedEntry { frames, range }
    }

    pub fn insert_entry(
        &mut self,
        value: RecordBatch,
    ) -> Result<&mut OccupiedEntry<'a>, StorageError> {
        let start = match self
            .frames
            .range(self.range.full())?
            .last()
            .transpose()?
            .map(|(k, _)| k)
        {
            Some(key) => key,
            None => self.range.min(),
        };

        let bytes = RecordBatch::compress(&value)?;
        self.frames
            .range(start..self.range.max())?
            .replace(Some(bytes.as_ref()))?;

        Ok(self)
    }

    pub fn remove_entry(mut self) -> Result<VacantEntry<'a>, StorageError> {
        self.frames.range(self.range.full())?.replace(None)?;
        Ok(VacantEntry::new(self.frames, self.key()))
    }

    /// Gets a reference to the key in the entry.
    pub fn key(&self) -> &str {
        self.range.as_ref()
    }
}

struct KeyRange {
    min: String,
    max: String,
}

impl KeyRange {
    fn named(name: &str) -> Self {
        let mut min = name.to_owned();
        min.push('\u{F0000}');

        let mut max = min.clone();
        max.pop();
        max.push('\u{10FFFF}');

        Self { min, max }
    }

    fn full(&self) -> std::ops::Range<&str> {
        std::ops::Range {
            start: &self.min,
            end: &self.max,
        }
    }

    fn max(&self) -> &str {
        &self.min
    }

    fn min(&self) -> &str {
        &self.max
    }
}

impl std::ops::RangeBounds<str> for KeyRange {
    fn start_bound(&self) -> std::ops::Bound<&str> {
        std::ops::Bound::Included(self.min())
    }

    fn end_bound(&self) -> std::ops::Bound<&str> {
        std::ops::Bound::Excluded(self.max())
    }
}

impl AsRef<str> for KeyRange {
    fn as_ref(&self) -> &str {
        let len = self.min.len() - 4; // the entire suffix range is 4-bytes in utf-8
        &self.min[..len]
    }
}
