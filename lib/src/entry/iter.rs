use crate::RecordBatch;
use crate::entry::OccupiedEntry;
use crate::{RangeInclusive, StorageError};
use itertools::{PutBack, put_back};
use loom::Predicate;

impl<'a> OccupiedEntry<'a> {
    pub fn iter(&'a self) -> Result<Iter<'a>, StorageError> {
        let inner = put_back(self.intervals()?);
        let projection = Vec::default();
        let predicate = Predicate::None;

        Ok(Iter {
            inner,
            projection,
            predicate,
        })
    }
}

impl<'a> Iter<'a> {
    pub fn projection(self, projection: &[String]) -> Self {
        Iter {
            projection: projection.into(),
            ..self
        }
    }

    pub fn predicate(self, predicate: Predicate) -> Self {
        Iter { predicate, ..self }
    }
}

pub struct Iter<'a> {
    pub(crate) inner: PutBack<RangeInclusive<'a>>,
    pub(crate) projection: Vec<String>,
    pub(crate) predicate: Predicate,
}

impl Iterator for Iter<'_> {
    type Item = Result<RecordBatch, StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next()? {
            Ok((_, value)) => Some(RecordBatch::decompress(
                &self.projection,
                &self.predicate,
                value,
            )),
            Err(err) => Some(Err(err.into())),
        }
    }
}
