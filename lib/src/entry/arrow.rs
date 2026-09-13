use crate::StorageError;
use arrow::compute::{concat_batches, filter_record_batch};
use arrow_schema::Schema;
use loom::compressors::FluxWriter;
use loom::decompressors::FluxReader;
use loom::{CompressionProfile, FluxError, LoomCompressor, LoomDecompressor, Predicate};
use std::borrow::Borrow;
use std::iter::once;
use std::ops::Deref;
use std::sync::Arc;

pub struct RecordBatch(arrow_array::RecordBatch);

impl RecordBatch {
    pub(crate) fn compress(batch: &RecordBatch) -> Result<impl AsRef<[u8]>, StorageError> {
        // Secondary compression is now done over LSM blocks, rather than `RecordBatch`s.
        // Unfortunately, this means that we no longer directly benefit from the ability
        // to skip (secondary) decompression by filtering over Flux/Atlas footers.
        let profile = CompressionProfile::Speed;
        let writer = FluxWriter::with_profile(profile).with_u64_only(true);
        writer.compress(batch).map_err(|err| match err {
            FluxError::Arrow(error) => error.into(),
            FluxError::Io(error) => error.into(),
            _ => StorageError::Compression(err.to_string()),
        })
    }

    pub(crate) fn decompress(
        projection: &[String],
        predicate: &Predicate,
        bytes: impl AsRef<[u8]>,
    ) -> Result<RecordBatch, StorageError> {
        let reader = FluxReader::new("");
        if projection.is_empty() {
            reader.decompress(bytes.as_ref(), predicate)
        } else {
            reader.decompress_projected(bytes.as_ref(), predicate, projection)
        }
        .and_then(|batch| {
            if matches!(predicate, Predicate::None) {
                Ok(batch)
            } else {
                let mask = predicate.eval_on_batch(&batch)?;
                filter_record_batch(&batch, &mask).map_err(|err| err.into())
            }
        })
        .map(RecordBatch)
        .map_err(|err| match err {
            FluxError::Arrow(error) => error.into(),
            FluxError::Io(error) => error.into(),
            _ => StorageError::Decompression(err.to_string()),
        })
    }
}

impl AsRef<<Self as Deref>::Target> for RecordBatch {
    #[inline]
    fn as_ref(&self) -> &<Self as Deref>::Target {
        self
    }
}

impl Borrow<<Self as Deref>::Target> for RecordBatch {
    #[inline]
    fn borrow(&self) -> &<Self as Deref>::Target {
        self
    }
}

impl Deref for RecordBatch {
    type Target = arrow_array::RecordBatch;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for RecordBatch {
    #[inline]
    fn default() -> Self {
        let schema = Arc::new(Schema::empty());
        RecordBatch(arrow_array::RecordBatch::new_empty(schema))
    }
}

impl<'a> Extend<&'a RecordBatch> for RecordBatch {
    /// # Panics
    /// Panics if the `RecordBatch`s within `I` do not have a matching schema.
    #[track_caller]
    fn extend<I: IntoIterator<Item = &'a RecordBatch>>(&mut self, iter: I) {
        let schema = self.schema();
        self.0 = concat_batches(
            &schema, //
            once(&self.0).chain(iter.into_iter().map(|x| &x.0)),
        )
        .unwrap();
    }
}

impl From<<Self as Deref>::Target> for RecordBatch {
    fn from(value: <Self as Deref>::Target) -> Self {
        RecordBatch(value)
    }
}
