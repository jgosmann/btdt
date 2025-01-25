use crate::cache::blob_id::BlobId;
use chrono::{DateTime, Utc};
use rkyv::{AlignedVec, Archive, Serialize};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;

#[derive(Archive, Clone, Debug, Serialize, PartialEq)]
#[archive(check_bytes, compare(PartialEq))]
#[archive_attr(derive(Debug))]
struct MetaV1 {
    version: u16,
    blob_id: BlobId,
    latest_access: i64,
    latest_access_nsecs: u32,
}

const _META_V1_SCRATCH_SIZE: usize = 0;

impl MetaV1 {
    pub fn new(blob_id: BlobId, latest_access: DateTime<Utc>) -> Self {
        Self {
            version: 1,
            blob_id,
            latest_access: latest_access.timestamp(),
            latest_access_nsecs: latest_access.timestamp_subsec_nanos(),
        }
    }
}

pub const META_MAX_SIZE: usize = 32;

#[derive(Debug)]
pub struct Meta<T> {
    data: T,
    archive_view: NonNull<ArchivedMetaV1>,
    _pin: PhantomPinned,
}

impl Meta<AlignedVec> {
    pub fn new(blob_id: BlobId, latest_access: DateTime<Utc>) -> Pin<Box<Self>> {
        let meta = MetaV1::new(blob_id, latest_access);
        let data = rkyv::to_bytes::<MetaV1, _META_V1_SCRATCH_SIZE>(&meta)
            .expect("failed to serialize meta");
        let mut boxed_meta = Box::new(Self {
            data,
            archive_view: NonNull::dangling(),
            _pin: PhantomPinned,
        });
        // Safety: we just serialized the data
        boxed_meta.archive_view =
            NonNull::from(unsafe { rkyv::archived_root::<MetaV1>(&boxed_meta.data) });
        Box::into_pin(boxed_meta)
    }
}

impl<T: AsMut<[u8]>> Meta<T> {
    pub fn from_bytes(data: T) -> Result<Pin<Box<Self>>, DeserializationError<impl Debug>> {
        let mut boxed_meta = Box::new(Self {
            data,
            archive_view: NonNull::dangling(),
            _pin: PhantomPinned,
        });
        let meta = rkyv::check_archived_root::<MetaV1>(boxed_meta.data.as_mut())?;
        boxed_meta.archive_view = NonNull::from(meta);
        Ok(Box::into_pin(boxed_meta))
    }

    pub fn set_latest_access(self: &mut Pin<Box<Self>>, latest_access: DateTime<Utc>) {
        // Safety: we're not moving the data out of the pin.
        let x = unsafe { self.as_mut().get_unchecked_mut() };
        // Safety: self.archive_view is always a valid pointer after initialization
        let archive_view = unsafe { x.archive_view.as_mut() };
        archive_view.latest_access = latest_access.timestamp();
        archive_view.latest_access_nsecs = latest_access.timestamp_subsec_nanos();
    }
}

impl<T: AsRef<[u8]>> Meta<T> {
    pub fn blob_id(&self) -> &BlobId {
        // Safety: self.archive_view is always a valid pointer after initialization
        let archive_view = unsafe { self.archive_view.as_ref() };
        &archive_view.blob_id
    }

    pub fn latest_access(&self) -> Result<DateTime<Utc>, DeserializationError<()>> {
        // Safety: self.archive_view is always a valid pointer after initialization
        let archive_view = unsafe { self.archive_view.as_ref() };
        DateTime::from_timestamp(archive_view.latest_access, archive_view.latest_access_nsecs)
            .ok_or(DeserializationError::from(()))
    }
}

impl<T: AsRef<[u8]>> AsRef<[u8]> for Meta<T> {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

#[derive(Debug)]
pub struct DeserializationError<C: Debug> {
    _cause: C,
}

impl<C: Debug> From<C> for DeserializationError<C> {
    fn from(cause: C) -> Self {
        Self { _cause: cause }
    }
}

impl<C: Debug> Display for DeserializationError<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Deserialization error")
    }
}

impl<C: Debug> Error for DeserializationError<C> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::blob_id::BLOB_ID_SIZE;
    use rkyv::ser::serializers::{
        AlignedSerializer, AllocScratch, CompositeSerializer, FallbackScratch, HeapScratch,
        ScratchTracker, SharedSerializeMap,
    };
    use rkyv::ser::Serializer;
    use rkyv::AlignedVec;
    use std::ops::{Add, Deref};

    #[test]
    fn test_meta_v1_scratch_size() {
        let meta = MetaV1::new(
            [0; BLOB_ID_SIZE],
            DateTime::parse_from_rfc3339("2025-01-24T20:47:33.123Z")
                .unwrap()
                .to_utc(),
        );

        let tracker = ScratchTracker::new(FallbackScratch::new(
            HeapScratch::<_META_V1_SCRATCH_SIZE>::new(),
            AllocScratch::new(),
        ));
        let mut serializer = Box::new(CompositeSerializer::new(
            AlignedSerializer::<AlignedVec>::default(),
            tracker,
            SharedSerializeMap::default(),
        ));
        serializer.serialize_value(&meta).unwrap();
        let (_, tracker, _) = serializer.into_components();
        assert_eq!(tracker.max_bytes_allocated(), _META_V1_SCRATCH_SIZE);
    }

    #[test]
    fn test_meta_stores_values_passed_in_constructor() {
        let blob_id = b"0123456789012345";
        let date = DateTime::parse_from_rfc3339("2025-01-24T20:47:33.123Z")
            .unwrap()
            .to_utc();
        let meta = Meta::new(blob_id.to_owned(), date);
        assert_eq!(meta.blob_id(), blob_id);
        assert_eq!(meta.latest_access().unwrap(), date);
    }

    #[test]
    fn test_can_set_latest_access_date() {
        let mut date = DateTime::parse_from_rfc3339("2025-01-24T20:47:33.123Z")
            .unwrap()
            .to_utc();
        let mut meta = Meta::new([0; BLOB_ID_SIZE], date);
        date = date.add(chrono::Duration::days(1));
        meta.set_latest_access(date);
        assert_eq!(meta.latest_access().unwrap(), date);
    }

    #[test]
    fn test_meta_roundtrip() {
        let meta_in = Meta::new(
            [0; BLOB_ID_SIZE],
            DateTime::parse_from_rfc3339("2025-01-24T20:47:33.123Z")
                .unwrap()
                .to_utc(),
        );
        let data = Vec::from(meta_in.deref().as_ref());
        let meta_out = Meta::from_bytes(data).unwrap();
        assert_eq!(meta_in.blob_id(), meta_out.blob_id());
        assert_eq!(
            meta_in.latest_access().unwrap(),
            meta_out.latest_access().unwrap()
        );
    }

    #[test]
    fn test_meta_max_size_is_accurate() {
        let meta = Meta::new(
            [0; BLOB_ID_SIZE],
            DateTime::parse_from_rfc3339("2025-01-24T20:47:33.123Z")
                .unwrap()
                .to_utc(),
        );
        let serialized_size = meta.deref().as_ref().len();
        assert_eq!(
            serialized_size, META_MAX_SIZE,
            "Set META_MAX_SIZE (currently {}) to {}, the correct serialized size of Meta",
            serialized_size, META_MAX_SIZE
        );
    }
}
