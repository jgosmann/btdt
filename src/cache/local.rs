use super::blob_id::{BlobId, BlobIdFactory};
use super::meta::{Meta, META_MAX_SIZE};
use super::Cache;
use crate::storage::Storage;
use crate::util::clock::{Clock, SystemClock};
use crate::util::close::Close;
use crate::util::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use rkyv::util::AlignedVec;
use std::cell::RefCell;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::ops::Deref;
use std::pin::Pin;

pub struct LocalCache<S: Storage, C: Clock = SystemClock> {
    storage: RefCell<S>,
    blob_id_factory: BlobIdFactory,
    clock: C,
}

impl<S: Storage> LocalCache<S> {
    pub fn new(storage: S) -> Self {
        Self::with_clock(storage, SystemClock)
    }
}

impl<S: Storage, C: Clock> LocalCache<S, C> {
    pub fn with_clock(storage: S, clock: C) -> Self {
        Self {
            storage: RefCell::new(storage),
            blob_id_factory: BlobIdFactory::default(),
            clock,
        }
    }

    pub fn into_storage(self) -> S {
        self.storage.into_inner()
    }

    fn blob_path(blob_id: &BlobId) -> String {
        let blob_id = ICASE_NOPAD_ALPHANUMERIC_ENCODING.encode(blob_id.as_ref());
        format!("/blob/{}/{}", &blob_id[..2], &blob_id[2..])
    }

    fn meta_path(key: &str) -> String {
        // Use a hash of the key to avoid too many files in a single directory
        let hash =
            ICASE_NOPAD_ALPHANUMERIC_ENCODING.encode(&blake3::hash(key.as_bytes()).as_bytes()[..2]);
        format!("/meta/{}/{}", hash, key)
    }
}

impl<S: Storage, C: Clock> Cache for LocalCache<S, C> {
    type Reader = S::Reader;
    type Writer = CacheWriter<S, AlignedVec>;

    fn get(&self, keys: &[&str]) -> io::Result<Option<Self::Reader>> {
        for key in keys {
            let meta_path = Self::meta_path(key);
            let entry = self.storage.borrow().get(&meta_path);
            match entry {
                Ok(mut reader) => {
                    let mut meta_data = [0u8; META_MAX_SIZE];
                    reader.read_exact(meta_data.as_mut())?;
                    let mut meta = Meta::from_bytes(meta_data).map_err(|err| {
                        io::Error::new(ErrorKind::InvalidData, format!("{:?}", err))
                    })?;

                    meta.set_latest_access(self.clock.now());
                    let mut writer = self.storage.borrow_mut().put(&meta_path)?;
                    writer.write_all(meta.deref().as_ref())?;
                    writer.close()?;

                    let blob_path = Self::blob_path(meta.blob_id());
                    return Ok(Some(self.storage.borrow().get(&blob_path)?));
                }
                Err(err) => match err.kind() {
                    ErrorKind::NotFound => continue,
                    _ => return Err(err),
                },
            }
        }
        Ok(None)
    }

    fn set(&mut self, keys: &[&str]) -> io::Result<Self::Writer> {
        let blob_id = self.blob_id_factory.new_id();
        let meta = Meta::new(blob_id, self.clock.now());
        let blob_path = Self::blob_path(&blob_id);
        let blob_writer = self.storage.borrow_mut().put(&blob_path)?;
        let meta_writers = keys
            .iter()
            .map(|&key| Self::meta_path(key))
            .map(|key| self.storage.borrow_mut().put(&key))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(CacheWriter::new(blob_writer, meta_writers, meta))
    }

    fn clean(&mut self) -> io::Result<()> {
        todo!()
    }
}

pub struct CacheWriter<S: Storage, M: AsRef<[u8]>> {
    blob_writer: S::Writer,
    meta_writers: Vec<S::Writer>,
    meta: Pin<Box<Meta<M>>>,
}

impl<S: Storage, M: AsRef<[u8]>> CacheWriter<S, M> {
    pub fn new(
        blob_writer: S::Writer,
        meta_writers: Vec<S::Writer>,
        meta: Pin<Box<Meta<M>>>,
    ) -> Self {
        CacheWriter {
            blob_writer,
            meta_writers,
            meta,
        }
    }
}

impl<S: Storage, M: AsRef<[u8]>> Write for CacheWriter<S, M> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.blob_writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.blob_writer.flush()
    }
}

impl<S: Storage, M: AsRef<[u8]>> Close for CacheWriter<S, M> {
    fn close(self) -> io::Result<()> {
        self.blob_writer.close()?;
        for mut writer in self.meta_writers {
            writer.write_all(self.meta.deref().as_ref())?;
            writer.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::in_memory::InMemoryStorage;
    use crate::util::clock::test_fakes::ControlledClock;
    use chrono::{DateTime, TimeDelta};

    #[test]
    fn test_returns_none_for_non_existent_keys() {
        let storage = InMemoryStorage::new();
        let cache = LocalCache::new(storage);
        let result = cache.get(&["non-existent-key", "another-non-existent-key"]);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_roundtrip() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        let mut writer = cache.set(&["key"]).unwrap();
        writer.write_all(b"Hello, world!").unwrap();
        writer.close().unwrap();
        let mut reader = cache.get(&["key"]).unwrap().unwrap();
        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "Hello, world!");
    }

    #[test]
    fn test_can_retrieve_cached_data_from_all_set_keys() {
        let keys = ["key0", "key1"];

        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        let mut writer = cache.set(&keys).unwrap();
        writer.write_all(b"Hello, world!").unwrap();
        writer.close().unwrap();

        for key in keys {
            let mut reader = cache.get(&[key]).unwrap().unwrap();
            let mut buf = String::new();
            reader.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "Hello, world!");
        }
    }

    #[test]
    fn test_get_falls_back_to_first_available_key() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);

        let mut writer = cache.set(&["actual-key"]).unwrap();
        writer.write_all(b"Hello, world!").unwrap();
        writer.close().unwrap();

        let mut writer = cache.set(&["ignored-key"]).unwrap();
        writer.write_all(b"Goodbye, world!").unwrap();
        writer.close().unwrap();

        let mut reader = cache
            .get(&["non-existent-key", "actual-key", "ignored-key"])
            .unwrap()
            .unwrap();
        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "Hello, world!");
    }

    #[test]
    fn test_get_updates_last_access_time() {
        let mut clock = ControlledClock::new(
            DateTime::parse_from_rfc3339("2025-01-02T03:04:05Z")
                .unwrap()
                .to_utc(),
        );
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        let mut writer = cache.set(&["key"]).unwrap();
        writer.write_all(b"Hello, world!").unwrap();
        writer.close().unwrap();

        clock.advance_by(TimeDelta::days(1));
        let mut reader = cache.get(&["key"]).unwrap().unwrap();
        reader.read_to_string(&mut String::new()).unwrap();

        let storage = cache.into_storage();
        let mut meta_reader = storage
            .get(&LocalCache::<InMemoryStorage>::meta_path("key"))
            .unwrap();
        let mut buf = Vec::with_capacity(META_MAX_SIZE);
        meta_reader.read_to_end(&mut buf).unwrap();
        let meta = Meta::from_bytes(&mut buf).unwrap();
        assert_eq!(meta.deref().latest_access().unwrap(), clock.now());
    }
}
