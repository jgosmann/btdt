use super::blob_id::{BlobId, BlobIdFactory};
use super::meta::{Meta, META_MAX_SIZE};
use super::Cache;
use crate::close::Close;
use crate::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use crate::storage::Storage;
use chrono::Utc;
use rkyv::AlignedVec;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::ops::Deref;
use std::pin::Pin;

pub struct LocalCache<S: Storage> {
    storage: S,
    blob_id_factory: BlobIdFactory,
}

impl<S: Storage> LocalCache<S> {
    pub fn new(storage: S) -> Self {
        LocalCache {
            storage,
            blob_id_factory: BlobIdFactory::default(),
        }
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

impl<S: Storage> Cache for LocalCache<S> {
    type Reader = S::Reader;
    type Writer = CacheWriter<S, AlignedVec>;

    fn get(&self, keys: &[&str]) -> io::Result<Option<Self::Reader>> {
        // TODO update access time
        for key in keys {
            match self.storage.get(&Self::meta_path(key)) {
                Ok(mut reader) => {
                    let mut meta_data = [0u8; META_MAX_SIZE];
                    reader.read_exact(meta_data.as_mut())?;
                    let meta = Meta::from_bytes(meta_data).map_err(|err| {
                        io::Error::new(ErrorKind::InvalidData, format!("{:?}", err))
                    })?;
                    let blob_path = Self::blob_path(meta.blob_id());
                    return Ok(Some(self.storage.get(&blob_path)?));
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
        let meta = Meta::new(blob_id, Utc::now());
        let blob_path = Self::blob_path(&blob_id);
        let blob_writer = self.storage.put(&blob_path)?;
        let meta_writers = keys
            .iter()
            .map(|&key| Self::meta_path(key))
            .map(|key| self.storage.put(&key))
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
}
