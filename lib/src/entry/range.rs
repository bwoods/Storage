use crate::entry::OccupiedEntry;
use crate::entry::iter::Iter;
use crate::{RecordBatch, StorageError};
use loom::Predicate;
use loom::atlas::AtlasFooter;
use serde::{Deserialize, de::DeserializeOwned};
use serde_arrow::Deserializer;
use std::collections::{BTreeMap, VecDeque};
use std::ops::{Bound, RangeBounds};

impl OccupiedEntry<'_> {
    pub fn range<T: DeserializeOwned>(
        &self,
        bounds: impl RangeBounds<usize>,
    ) -> Result<impl Iterator<Item = Result<T, StorageError>>, StorageError> {
        self.restrict(bounds, &[], Predicate::None)
    }

    pub fn restrict<T: DeserializeOwned>(
        &self,
        bounds: impl RangeBounds<usize>,
        projection: &[String],
        predicate: Predicate,
    ) -> Result<impl Iterator<Item = Result<T, StorageError>>, StorageError> {
        let iter = self.iter()?.projection(projection).predicate(predicate);

        let mut n = match bounds.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        Ok(Range {
            iter,
            queue: Default::default(),
        }
        // using `dropping` here, rather than `skip`, would eagerly fill `queue`
        // on iterator creation and that feels… un-Rust-like?
        .skip(n)
        .take_while(move |_| {
            let keep = bounds.contains(&n);
            n += 1;
            keep
        }))
    }

    #[inline(never)]
    pub fn remove(&mut self, bounds: impl RangeBounds<usize>) -> Result<(), StorageError> {
        let mut iter = self.iter()?;
        let mut updates = BTreeMap::default();

        let mut range = {
            let start = match bounds.start_bound() {
                Bound::Unbounded => 0,
                Bound::Included(min) => *min,
                Bound::Excluded(min) => *min + 1,
            };

            let end = match bounds.end_bound() {
                Bound::Unbounded => usize::MAX,
                Bound::Included(max) => *max + 1, // wrapping → empty range
                Bound::Excluded(max) => *max,
            };

            start..end
        };

        let mut start = iter.advance_inner(range.start)?;

        loop {
            if range.is_empty() {
                break;
            }

            let key = match iter.inner.next().transpose()? {
                None => break,
                Some(found) => {
                    let key = found.0;
                    iter.inner.put_back(Ok(found));
                    key
                }
            };

            let mut batch = match iter.next() {
                None => break,
                Some(Ok(batch)) => batch,
                Some(Err(err)) => return Err(err),
            };

            let length = usize::min(range.len(), batch.num_rows() - start);
            batch.slice(start, length);
            start = 0; // after this we always start at the beginning of a block

            if batch.num_rows() == range.len() {
                updates.insert(key, None); // remove this batch entirely
            } else {
                let bytes = RecordBatch::compress(&batch)?;
                updates.insert(key, Some(bytes)); // replace this batch (with a subslice)
            }

            range.start += length;
        }

        drop(iter); // now that `iter` is no longer borrowing `self`, we can perform the (delayed) updates

        for (key, replacement) in updates {
            self.frames.range(key..=key)?.replace(replacement)?;
        }

        Ok(())
    }
}

struct Range<'a, T> {
    queue: VecDeque<T>,
    iter: Iter<'a>,
}

impl<T> Iterator for Range<'_, T>
where
    T: DeserializeOwned,
{
    type Item = Result<T, StorageError>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self.advance_by(n) {
            Err(err) => Some(Err(err)),
            Ok(0) => Ok(self.queue.pop_front()).transpose(),
            Ok(_) => None,
        }
    }
}

impl<T> Range<'_, T>
where
    T: DeserializeOwned,
{
    #[inline(never)]
    fn advance_by(&mut self, mut n: usize) -> Result<usize, StorageError> {
        if self.queue.is_empty() == false {
            if n < self.queue.len() {
                if n > 0 {
                    self.queue.drain(..n).for_each(drop);
                }

                return Ok(0);
            } else {
                n -= self.queue.len();
                self.queue.clear();
            }
        }

        let k = self.iter.advance_inner(n)?;
        let mut batch = match self.iter.next().transpose()? {
            None => return Ok(k),
            Some(batch) => batch,
        };

        if k != 0 {
            batch.slice(k, batch.num_rows() - k); // “Returns a zero-copy slice of this array…”
        }

        let deserializer = Deserializer::from_record_batch(&batch)
            .map_err(|err| StorageError::Deserialization(err))?;

        self.queue.append(
            &mut VecDeque::<T>::deserialize(deserializer)
                .map_err(|err| StorageError::Deserialization(err))?,
        );

        Ok(0)
    }
}

impl Iter<'_> {
    /// Advances the `Iter` until its `next()` will return the block that holds row `n` + 1.
    ///
    /// Returns the gap between `n` and the number of rows that were actually skipped.
    #[inline(never)]
    fn advance_inner(&mut self, n: usize) -> Result<usize, StorageError> {
        let mut skipped: usize = 0;

        loop {
            match self.inner.next() {
                Some(Ok(found)) => {
                    let footer = AtlasFooter::from_file_tail(found.1)
                        .map_err(|err| StorageError::Decompression(err.to_string()))?;

                    let count: usize = footer
                        .blocks
                        .iter()
                        .map(|block| block.value_count as usize)
                        .sum();

                    if skipped + count > n {
                        self.inner.put_back(Ok(found));
                        return Ok(0);
                    } else {
                        skipped += count;
                        continue;
                    }
                }
                Some(Err(err)) => return Err(err.into()),
                None => return Ok(n - skipped),
            }
        }
    }
}
