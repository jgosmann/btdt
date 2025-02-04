use super::blob_id::{BlobId, BlobIdFactory};
use super::meta::{Meta, META_MAX_SIZE};
use super::Cache;
use crate::storage::{EntryType, Storage};
use crate::util::clock::{Clock, SystemClock};
use crate::util::close::Close;
use crate::util::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use chrono::{DateTime, TimeDelta, Utc};
use rkyv::util::AlignedVec;
use std::cell::{Ref, RefCell};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
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

    pub fn with_blob_id_factory(storage: S, blob_id_factory: BlobIdFactory) -> Self {
        Self {
            storage: RefCell::new(storage),
            blob_id_factory,
            clock: SystemClock,
        }
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
            ICASE_NOPAD_ALPHANUMERIC_ENCODING.encode(&blake3::hash(key.as_bytes()).as_bytes()[..1]);
        format!("/meta/{}/{}", hash, key)
    }
}

impl<S: Storage, C: Clock> Cache for LocalCache<S, C> {
    type Reader = S::Reader;
    type Writer = CacheWriter<S, AlignedVec>;

    fn get(&self, keys: &[&str]) -> io::Result<Option<Self::Reader>> {
        for key in keys {
            let meta_path = Self::meta_path(key);
            let meta = self.read_meta(&meta_path);
            match meta {
                Ok(mut meta) => {
                    meta.set_latest_access(self.clock.now());
                    let mut writer = self.storage.borrow_mut().put(&meta_path)?;
                    writer.write_all(meta.deref().as_ref())?;
                    writer.close()?;

                    let blob_path = Self::blob_path(meta.blob_id());
                    match self.storage.borrow().get(&blob_path) {
                        Ok(reader) => return Ok(Some(reader)),
                        Err(err) => match err.kind() {
                            ErrorKind::NotFound => continue,
                            _ => return Err(err),
                        },
                    }
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
        max_blob_size_sum: Option<u64>,
    ) -> io::Result<()> {
        if max_unused_age.is_none() && max_blob_size_sum.is_none() {
            return Ok(());
        }

        let mut blob_sizes = HashMap::new();
        {
            let storage = self.storage.borrow();
            for blob in Self::iter_subdir_files(&storage, "/blob")? {
                let blob = blob?;
                if let Ok(blob_id) = ICASE_NOPAD_ALPHANUMERIC_ENCODING
                    .decode(format!("{}{}", blob.subdir, blob.name).as_bytes())
                {
                    let blob_id: BlobId = blob_id.try_into().unwrap();
                    blob_sizes.insert(blob_id, blob.size);
                }
            }
        }

        #[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
        struct Blob {
            latest_access: Reverse<DateTime<Utc>>,
            size: u64,
            blob_id: BlobId,
            keys: Vec<String>,
        }
        let mut blobs: HashMap<BlobId, Blob> = HashMap::new();

        {
            let storage = self.storage.borrow();
            for key_file in Self::iter_subdir_files(&storage, "/meta")? {
                let key_file = key_file?;
                let meta = self.read_meta(&key_file.path)?;
                let latest_access = meta
                    .latest_access()
                    .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("{:?}", err)))?;
                if let Some(&size) = blob_sizes.get(meta.blob_id()) {
                    let entry = blobs.entry(*meta.blob_id()).or_insert_with(|| Blob {
                        latest_access: Reverse(latest_access),
                        size,
                        blob_id: *meta.blob_id(),
                        keys: vec![],
                    });
                    entry.keys.push(key_file.name.to_string());
                    entry.latest_access =
                        Reverse(std::cmp::max(entry.latest_access.0, latest_access));
                }
            }
        }

        let mut blob_size_sum: u64 = blobs.values().map(|blob| blob.size).sum();
        let mut heap: BinaryHeap<Blob> = blobs.into_values().collect();

        let cutoff = max_unused_age.map(|max_unused_age| self.clock.now() - max_unused_age);
        while !heap.is_empty() {
            if let Some(Blob {
                latest_access: Reverse(latest_access),
                ..
            }) = heap.peek()
            {
                if latest_access >= &cutoff.unwrap_or(DateTime::<Utc>::MIN_UTC)
                    && blob_size_sum <= max_blob_size_sum.unwrap_or(u64::MAX)
                {
                    break;
                }
            }
            let Blob {
                keys,
                blob_id,
                size,
                ..
            } = heap.pop().unwrap();
            for key in keys {
                self.storage.borrow_mut().delete(&Self::meta_path(&key))?;
            }
            self.storage
                .borrow_mut()
                .delete(&Self::blob_path(&blob_id))?;
            blob_size_sum -= size;
        }

        Ok(())
    }

    fn read_meta(&self, path: &str) -> io::Result<Pin<Box<Meta<[u8; META_MAX_SIZE]>>>> {
        let mut reader = self.storage.borrow().get(path)?;
        let mut meta_data = [0u8; META_MAX_SIZE];
        reader.read_exact(meta_data.as_mut())?;
        Meta::from_bytes(meta_data)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("{:?}", err)))
    }

    fn iter_subdir_files<'a>(
        storage: &'a Ref<S>,
        path: &'a str,
    ) -> io::Result<impl Iterator<Item = io::Result<SubdirFile>> + use<'a, S, C>> {
        let path_entries = storage.list(path)?.collect::<io::Result<Vec<_>>>()?;
        Ok(path_entries.into_iter().flat_map(move |path_entry| {
            if path_entry.entry_type != EntryType::Directory {
                return vec![].into_iter();
            }

            let subdir_path = format!("{}/{}", path, path_entry.name);
            let subdir_entries = storage.list(&subdir_path);
            match subdir_entries {
                Ok(subdir_entries) => subdir_entries
                    .filter_map(|subdir_entry| match subdir_entry {
                        Ok(subdir_entry) => {
                            if subdir_entry.entry_type != EntryType::File {
                                return None;
                            }

                            Some(Ok(SubdirFile {
                                path: format!("{}/{}", subdir_path, subdir_entry.name),
                                subdir: path_entry.name.to_string(),
                                name: subdir_entry.name.to_string(),
                                size: subdir_entry.size,
                            }))
                        }
                        Err(err) => Some(Err(err)),
                    })
                    .collect::<Vec<_>>()
                    .into_iter(),
                Err(err) => vec![Err(err)].into_iter(),
            }
        }))
    }
}

struct SubdirFile {
    path: String,
    subdir: String,
    name: String,
    size: u64,
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
        let mut clock = ControlledClock::default();
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
        let mut clock = ControlledClock::default();
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
        assert_blob_count(&storage, 1);
    }

    #[test]
    fn test_clean_does_not_remove_entries_if_another_recently_accessed_key_exists() {
        let mut clock = ControlledClock::default();
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["old", "new"], "Hello, world!").unwrap();
        clock.advance_by(TimeDelta::days(2));

        cache.get(&["new"]).unwrap().unwrap();
        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        assert_cache_entry_with_content(&cache, &["old"], "Hello, world!");
        assert_cache_entry_with_content(&cache, &["new"], "Hello, world!");
    }

    #[test]
    fn test_clean_removes_longest_unused_entries_until_space_limit_is_met() {
        let mut clock = ControlledClock::default();
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(
            &mut cache,
            &["3-days-old", "3-days-old-alternate-key"],
            "0123456789",
        )
        .unwrap();
        clock.advance_by(TimeDelta::days(1));
        cache_entry_with_content(&mut cache, &["2-days-old"], "0123456789").unwrap();
        clock.advance_by(TimeDelta::days(1));
        cache_entry_with_content(&mut cache, &["1-day-old"], "0123456789").unwrap();
        clock.advance_by(TimeDelta::days(1));
        cache_entry_with_content(&mut cache, &["0-days-old"], "0123456789").unwrap();

        cache.clean(None, Some(21)).unwrap();

        assert_no_cache_entry(
            &cache,
            &["3-days-old", "3-days-old-alternate-key", "2-days-old"],
        );
        assert_cache_entry_with_content(&cache, &["1-day-old"], "0123456789");
        assert_cache_entry_with_content(&cache, &["0-days-old"], "0123456789");

        let storage = cache.into_storage();
        assert_blob_count(&storage, 2);
    }

    #[test]
    fn test_key_without_blob_is_handled_gracefully() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &["key0"], "cached content").unwrap();

        let mut storage = cache.into_storage();
        let mut to_delete = Vec::new();
        for subdir in storage.list("/blob").unwrap() {
            let subdir = subdir.unwrap();
            for entry in storage.list(&format!("/blob/{}", subdir.name)).unwrap() {
                let entry = entry.unwrap();
                to_delete.push(format!("/blob/{}/{}", subdir.name, entry.name));
            }
        }
        for path in to_delete {
            storage.delete(&path).unwrap();
        }

        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &["key1"], "fallback").unwrap();

        assert!(cache.get(&["key0"]).unwrap().is_none());
        assert_cache_entry_with_content(&cache, &["key0", "key1"], "fallback");
    }

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

    fn assert_blob_count<S: Storage>(storage: &S, count: usize) {
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
            count,
            "Expected only {} blobs to remain",
            count
        );
    }
}
