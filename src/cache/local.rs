use super::blob_id::{BlobId, BlobIdFactory};
use super::meta::{Meta, META_MAX_SIZE};
use super::Cache;
use crate::storage::{EntryType, Storage};
use crate::util::clock::{Clock, SystemClock};
use crate::util::close::Close;
use crate::util::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use chrono::TimeDelta;
use rkyv::util::AlignedVec;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
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
}

impl<S: Storage, C: Clock> LocalCache<S, C> {
    pub fn clean(
        &mut self,
        max_unused_age: Option<TimeDelta>,
        max_blob_size_sum: Option<usize>,
    ) -> io::Result<()> {
        if max_unused_age.is_none() && max_blob_size_sum.is_none() {
            return Ok(());
        }

        let mut key_heap = BinaryHeap::new();

        {
            let storage = self.storage.borrow();
            let key_dirs = storage.list("/meta")?.collect::<io::Result<Vec<_>>>()?;
            for key_dir in key_dirs
                .iter()
                .filter(|key_dir| key_dir.entry_type == EntryType::Directory)
            {
                for key in storage.list(&format!("/meta/{}", key_dir.name))? {
                    let key = key?;
                    let mut reader = storage.get(&Self::meta_path(&key.name))?;
                    let mut meta_data = [0u8; META_MAX_SIZE];
                    reader.read_exact(meta_data.as_mut())?;
                    let meta = Meta::from_bytes(meta_data).map_err(|err| {
                        io::Error::new(ErrorKind::InvalidData, format!("{:?}", err))
                    })?;
                    key_heap.push((
                        Reverse(meta.latest_access().map_err(|err| {
                            io::Error::new(ErrorKind::InvalidData, format!("{:?}", err))
                        })?),
                        key.name.to_string(),
                        *meta.blob_id(),
                    ));
                }
            }
        }

        if let Some(max_unused_age) = max_unused_age {
            let cutoff = self.clock.now() - max_unused_age;
            while !key_heap.is_empty() {
                if let Some((Reverse(latest_access), _, _)) = key_heap.peek() {
                    if latest_access >= &cutoff {
                        break;
                    }
                }
                let (_, key, _) = key_heap.pop().unwrap();
                self.storage.borrow_mut().delete(&Self::meta_path(&key))?
            }
        }

        // garbage collect blobs
        let live_blobs: HashSet<BlobId> = key_heap.drain().map(|(_, _, blob_id)| blob_id).collect();
        let mut dead_blobs = Vec::with_capacity(live_blobs.len());
        {
            let storage = self.storage.borrow();
            let blob_dirs = storage.list("/blob")?.collect::<io::Result<Vec<_>>>()?;
            for blob_dir in blob_dirs {
                for blob in storage.list(&format!("/blob/{}", blob_dir.name))? {
                    let blob = blob?;
                    if let Ok(blob_id) = ICASE_NOPAD_ALPHANUMERIC_ENCODING
                        .decode(format!("{}{}", blob_dir.name, blob.name).as_bytes())
                    {
                        let blob_id: BlobId = blob_id.try_into().unwrap();
                        if !live_blobs.contains(&blob_id) {
                            dead_blobs.push(blob_id);
                        }
                    }
                }
            }
        }
        for blob_id in dead_blobs {
            self.storage
                .borrow_mut()
                .delete(&Self::blob_path(&blob_id))?;
        }

        Ok(())
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
        assert_no_cache_entry(&cache, &["non-existent-key", "another-non-existent-key"]);
    }

    #[test]
    fn test_roundtrip() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &["key"], "Hello, world!").unwrap();
        assert_cache_entry_with_content(&cache, &["key"], "Hello, world!");
    }

    #[test]
    fn test_can_retrieve_cached_data_from_all_set_keys() {
        let keys = ["key0", "key1"];

        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &keys, "Hello, world!").unwrap();

        for key in keys {
            assert_cache_entry_with_content(&cache, &[key], "Hello, world!");
        }
    }

    #[test]
    fn test_get_falls_back_to_first_available_key() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);

        cache_entry_with_content(&mut cache, &["actual-key"], "Hello, world!").unwrap();
        cache_entry_with_content(&mut cache, &["ignored-key"], "Goodbye, world!").unwrap();

        assert_cache_entry_with_content(
            &cache,
            &["non-existent-key", "actual-key", "ignored-key"],
            "Hello, world!",
        );
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

        cache_entry_with_content(&mut cache, &["key"], "Hello, world!").unwrap();

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

    #[test]
    fn test_clean_does_not_do_anything_if_no_limits_are_given() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);

        cache_entry_with_content(&mut cache, &["key"], "Hello, world!").unwrap();

        cache.clean(None, None).unwrap();

        assert_cache_entry_with_content(&cache, &["key"], "Hello, world!");
    }

    #[test]
    fn test_clean_removes_unused_entries() {
        let mut clock = ControlledClock::new(
            DateTime::parse_from_rfc3339("2025-01-02T03:04:05Z")
                .unwrap()
                .to_utc(),
        );
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["old"], "Hello, world!").unwrap();
        clock.advance_by(TimeDelta::days(2));
        cache_entry_with_content(&mut cache, &["new"], "Goodbye, world!").unwrap();
        clock.advance_by(TimeDelta::days(1));

        cache.clean(Some(TimeDelta::days(2)), None).unwrap();

        assert_no_cache_entry(&cache, &["old"]);
        assert_cache_entry_with_content(&cache, &["new"], "Goodbye, world!");

        let storage = cache.into_storage();
        let blob_dirs = storage
            .list("/blob")
            .unwrap()
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            blob_dirs
                .iter()
                .map(|blob_dir| {
                    storage
                        .list(&format!("/blob/{}", blob_dir.name))
                        .unwrap()
                        .count()
                })
                .sum::<usize>(),
            1,
            "Expected only one blob to remain"
        );
    }

    // TODO: test clean removes blobs exceeding cache size

    fn cache_entry_with_content<C: Cache>(
        cache: &mut C,
        keys: &[&str],
        content: &str,
    ) -> io::Result<()> {
        let mut writer = cache.set(keys)?;
        writer.write_all(content.as_bytes())?;
        writer.close()
    }

    fn assert_cache_entry_with_content<C: Cache>(cache: &C, keys: &[&str], content: &str) {
        let mut reader = cache
            .get(keys)
            .expect("IO failure getting cache entry")
            .expect("cache entry not found");
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .expect("failed to read cache entry");
        assert_eq!(buf, content, "cache entry content mismatch");
    }

    fn assert_no_cache_entry<C: Cache>(cache: &C, keys: &[&str]) {
        let result = cache.get(keys).expect("IO failure getting cache entry");
        assert!(result.is_none(), "unexpected cache entry found");
    }
}
