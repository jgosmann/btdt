//! Provides a local cache implementation that stores data in a storage backend.

use super::blob_id::{BlobId, BlobIdFactory, RngBytes, ThreadRng};
use super::meta::{META_MAX_SIZE, Meta};
use super::{Cache, CacheHit};
use crate::error::{IoPathError, IoPathResult, WithPath};
use crate::storage::{EntryType, Storage};
use crate::util::clock::{Clock, SystemClock};
use crate::util::close::Close;
use crate::util::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use chrono::{DateTime, TimeDelta, Utc};
use rkyv::util::AlignedVec;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::ops::Deref;
use std::pin::Pin;

/// A blob is written before the meta files referencing it. Unreferenced blobs younger than this
/// are kept because their meta files might still be in the process of being written.
const UNREFERENCED_BLOB_GRACE_PERIOD: TimeDelta = TimeDelta::hours(1);

/// A local cache that stores data in a storage backend.
///
/// Note that the storage backend itself could store data remotely, despite the cache being
/// implemented locally.
///
/// # Examples
///
/// ```rust
/// # use std::io;
/// use std::io::{Read, Write};
/// use btdt::cache::local::LocalCache;
/// # use btdt::error::IoPathResult;
/// use btdt::storage::in_memory::InMemoryStorage;
/// use btdt::util::close::Close;
///
/// # fn main() -> io::Result<()> {
/// use btdt::cache::Cache;
/// use btdt::error::WithPath;
/// let mut cache = LocalCache::new(InMemoryStorage::new());
/// let mut writer = cache.set(&["cache-key"])?;
/// writer.write_all(b"Hello, world!")?;
/// writer.close()?;
/// let mut buf = String::new();
/// cache.get(&["foo", "cache-key"])?.unwrap().reader.read_to_string(&mut buf)?;
/// assert_eq!(buf, "Hello, world!");
/// # Ok(())
/// # }
/// ```
pub struct LocalCache<S: Storage, C: Clock = SystemClock, R: RngBytes = ThreadRng> {
    storage: S,
    blob_id_factory: BlobIdFactory<R>,
    clock: C,
}
impl<S: Storage> LocalCache<S, SystemClock, ThreadRng> {
    /// Creates a new local cache that stores data in the given storage backend.
    pub fn new(storage: S) -> Self {
        Self::with_clock(storage, SystemClock)
    }
}

impl<S: Storage, R: RngBytes> LocalCache<S, SystemClock, R> {
    /// Creates a new local cache that stores data in the given storage backend, using the given
    /// blob ID factory.
    pub fn with_blob_id_factory(storage: S, blob_id_factory: BlobIdFactory<R>) -> Self {
        Self {
            storage,
            blob_id_factory,
            clock: SystemClock,
        }
    }
}

impl<S: Storage, C: Clock> LocalCache<S, C, ThreadRng> {
    /// Creates a new local cache that stores data in the given storage backend, using the given
    /// clock.
    pub(crate) fn with_clock(storage: S, clock: C) -> Self {
        Self {
            storage,
            blob_id_factory: BlobIdFactory::default(),
            clock,
        }
    }
}

impl<S: Storage, C: Clock, R: RngBytes> LocalCache<S, C, R> {
    /// Consumes the cache and returns the underlying storage.
    pub fn into_storage(self) -> S {
        self.storage
    }

    fn blob_path(blob_id: &BlobId) -> String {
        let blob_id = ICASE_NOPAD_ALPHANUMERIC_ENCODING.encode(blob_id.as_ref());
        format!("/blob/{}/{}", &blob_id[..2], &blob_id[2..])
    }

    fn meta_path(key: &str) -> String {
        // Use a hash of the key to avoid too many files in a single directory
        let hash =
            ICASE_NOPAD_ALPHANUMERIC_ENCODING.encode(&blake3::hash(key.as_bytes()).as_bytes()[..1]);
        format!("/meta/{hash}/{key}")
    }
}

impl<S: Storage, C: Clock, R: RngBytes> Cache for LocalCache<S, C, R> {
    type Reader = S::Reader;
    type Writer = CacheWriter<S, AlignedVec>;

    fn get<'a>(&self, keys: &[&'a str]) -> IoPathResult<Option<CacheHit<'a, Self::Reader>>> {
        for key in keys {
            let meta_path = Self::meta_path(key);
            let meta = self.read_meta(&meta_path);
            match meta {
                Ok(mut meta) => {
                    meta.set_latest_access(self.clock.now());
                    let mut writer = self.storage.put(&meta_path)?;
                    writer
                        .write_all(meta.deref().as_ref())
                        .with_path(&meta_path)?;
                    writer.close().with_path(&meta_path)?;

                    let blob_path = Self::blob_path(meta.blob_id());
                    match self.storage.get(&blob_path) {
                        Ok(file_handle) => {
                            return Ok(Some(CacheHit {
                                key,
                                reader: file_handle.reader,
                                size_hint: Some(file_handle.size_hint),
                            }));
                        }
                        Err(err) => match err.io_error().kind() {
                            ErrorKind::NotFound => continue,
                            _ => return Err(err),
                        },
                    }
                }
                Err(err) => match err.io_error().kind() {
                    ErrorKind::NotFound => continue,
                    _ => return Err(err),
                },
            }
        }
        Ok(None)
    }

    fn set(&self, keys: &[&str]) -> IoPathResult<Self::Writer> {
        let blob_id = self.blob_id_factory.new_id();
        let meta = Meta::new(blob_id, self.clock.now());
        let blob_path = Self::blob_path(&blob_id);
        let blob_writer = self.storage.put(&blob_path)?;
        let meta_writers = keys
            .iter()
            .map(|&key| Self::meta_path(key))
            .map(|key| self.storage.put(&key))
            .collect::<IoPathResult<Vec<_>>>()?;
        Ok(CacheWriter::new(blob_writer, meta_writers, meta))
    }
}

impl<S: Storage, C: Clock, R: RngBytes> LocalCache<S, C, R> {
    pub fn clean(
        &mut self,
        max_unused_age: Option<TimeDelta>,
        max_blob_size_sum: Option<u64>,
    ) -> IoPathResult<()> {
        if max_unused_age.is_none() && max_blob_size_sum.is_none() {
            return Ok(());
        }

        let mut blob_files = HashMap::new();
        for blob in Self::iter_subdir_files(&self.storage, "/blob")? {
            let blob = blob?;
            if let Ok(blob_id) = ICASE_NOPAD_ALPHANUMERIC_ENCODING
                .decode(format!("{}{}", blob.subdir, blob.name).as_bytes())
            {
                let blob_id: BlobId = blob_id.try_into().unwrap();
                blob_files.insert(blob_id, blob);
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

        for key_file in Self::iter_subdir_files(&self.storage, "/meta")? {
            let key_file = key_file?;
            let Some(meta) = ignore_not_found(self.read_meta(&key_file.path))? else {
                continue;
            };
            let latest_access = meta.latest_access().map_err(|err| {
                IoPathError::new_no_path(io::Error::new(ErrorKind::InvalidData, format!("{err:?}")))
            })?;
            if let Some(blob_file) = blob_files.get(meta.blob_id()) {
                let entry = blobs.entry(*meta.blob_id()).or_insert_with(|| Blob {
                    latest_access: Reverse(latest_access),
                    size: blob_file.size,
                    blob_id: *meta.blob_id(),
                    keys: vec![],
                });
                entry.keys.push(key_file.name.to_string());
                entry.latest_access = Reverse(std::cmp::max(entry.latest_access.0, latest_access));
            }
        }

        let unreferenced_cutoff = self.clock.now() - UNREFERENCED_BLOB_GRACE_PERIOD;
        for (blob_id, blob_file) in &blob_files {
            if !blobs.contains_key(blob_id)
                && blob_file
                    .modified
                    .is_some_and(|modified| modified < unreferenced_cutoff)
            {
                ignore_not_found(self.storage.delete(&blob_file.path))?;
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
                && latest_access >= &cutoff.unwrap_or(DateTime::<Utc>::MIN_UTC)
                && blob_size_sum <= max_blob_size_sum.unwrap_or(u64::MAX)
            {
                break;
            }
            let Blob {
                keys,
                blob_id,
                size,
                ..
            } = heap.pop().unwrap();
            for key in keys {
                ignore_not_found(self.storage.delete(&Self::meta_path(&key)))?;
            }
            ignore_not_found(self.storage.delete(&Self::blob_path(&blob_id)))?;
            blob_size_sum -= size;
        }

        Ok(())
    }

    fn read_meta(&self, path: &str) -> IoPathResult<Pin<Box<Meta<[u8; META_MAX_SIZE]>>>> {
        let mut reader = self.storage.get(path)?.reader;
        let mut meta_data = [0u8; META_MAX_SIZE];
        reader.read_exact(meta_data.as_mut()).no_path()?;
        Meta::from_bytes(meta_data).map_err(|err| {
            IoPathError::new_no_path(io::Error::new(ErrorKind::InvalidData, format!("{err:?}")))
        })
    }

    fn iter_subdir_files<'a>(
        storage: &'a S,
        path: &'a str,
    ) -> IoPathResult<impl Iterator<Item = IoPathResult<SubdirFile>> + use<'a, S, C, R>> {
        let path_entries = storage.list(path)?.collect::<IoPathResult<Vec<_>>>()?;
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
                                modified: subdir_entry.modified,
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
    modified: Option<DateTime<Utc>>,
}

/// Treats a file that no longer exists as success, as another process cleaning the same cache may
/// have deleted it already.
fn ignore_not_found<T>(result: IoPathResult<T>) -> IoPathResult<Option<T>> {
    match result {
        Err(err) if err.io_error().kind() == ErrorKind::NotFound => Ok(None),
        result => result.map(Some),
    }
}

/// A writer for a cache entry.
pub struct CacheWriter<S: Storage, M: AsRef<[u8]>> {
    blob_writer: S::Writer,
    meta_writers: Vec<S::Writer>,
    meta: Pin<Box<Meta<M>>>,
}

impl<S: Storage, M: AsRef<[u8]>> CacheWriter<S, M> {
    fn new(blob_writer: S::Writer, meta_writers: Vec<S::Writer>, meta: Pin<Box<Meta<M>>>) -> Self {
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
    use crate::storage::{FileHandle, StorageEntry};
    use crate::util::clock::test_fakes::ControlledClock;
    use chrono::TimeDelta;

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
        assert_cache_entry_with_content(&cache, &["key"], "key", "Hello, world!");
    }

    #[test]
    fn test_can_retrieve_cached_data_from_all_set_keys() {
        let keys = ["key0", "key1"];

        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &keys, "Hello, world!").unwrap();

        for key in keys {
            assert_cache_entry_with_content(&cache, &[key], key, "Hello, world!");
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
            "actual-key",
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
        let mut reader = cache.get(&["key"]).unwrap().unwrap().reader;
        reader.read_to_string(&mut String::new()).unwrap();

        let storage = cache.into_storage();
        let mut meta_reader = storage
            .get(&LocalCache::<InMemoryStorage>::meta_path("key"))
            .unwrap();
        let mut buf = Vec::with_capacity(META_MAX_SIZE);
        meta_reader.reader.read_to_end(&mut buf).unwrap();
        let meta = Meta::from_bytes(&mut buf).unwrap();
        assert_eq!(meta.deref().latest_access().unwrap(), clock.now());
    }

    #[test]
    fn test_clean_does_not_do_anything_if_no_limits_are_given() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);

        cache_entry_with_content(&mut cache, &["key"], "Hello, world!").unwrap();

        cache.clean(None, None).unwrap();

        assert_cache_entry_with_content(&cache, &["key"], "key", "Hello, world!");
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
        assert_cache_entry_with_content(&cache, &["new"], "new", "Goodbye, world!");

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

        assert_cache_entry_with_content(&cache, &["old"], "old", "Hello, world!");
        assert_cache_entry_with_content(&cache, &["new"], "new", "Hello, world!");
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
        assert_cache_entry_with_content(&cache, &["1-day-old"], "1-day-old", "0123456789");
        assert_cache_entry_with_content(&cache, &["0-days-old"], "0-days-old", "0123456789");

        let storage = cache.into_storage();
        assert_blob_count(&storage, 2);
    }

    #[test]
    fn test_clean_removes_unreferenced_blobs() {
        let mut clock = ControlledClock::new(Utc::now());
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["key"], "old content").unwrap();
        cache_entry_with_content(&mut cache, &["key"], "new content").unwrap();
        clock.advance_by(TimeDelta::hours(2));

        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        assert_cache_entry_with_content(&cache, &["key"], "key", "new content");
        let storage = cache.into_storage();
        assert_blob_count(&storage, 1);
    }

    #[test]
    fn test_clean_keeps_recently_written_unreferenced_blobs() {
        let mut clock = ControlledClock::new(Utc::now());
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["key"], "old content").unwrap();
        cache_entry_with_content(&mut cache, &["key"], "new content").unwrap();
        clock.advance_by(TimeDelta::minutes(30));

        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        let storage = cache.into_storage();
        assert_blob_count(&storage, 2);
    }

    #[test]
    fn test_clean_tolerates_unreferenced_blobs_deleted_concurrently() {
        let mut clock = ControlledClock::new(Utc::now());
        let storage = ConcurrentlyDeletingStorage::new(StorageOperation::Delete);
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["unused"], "unused content").unwrap();
        cache_entry_with_content(&mut cache, &["key"], "old content").unwrap();
        cache_entry_with_content(&mut cache, &["key"], "new content").unwrap();
        clock.advance_by(TimeDelta::days(2));

        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        assert_no_cache_entry(&cache, &["unused"]);
    }

    #[test]
    fn test_clean_tolerates_evicted_entries_deleted_concurrently() {
        let mut clock = ControlledClock::default();
        let storage = ConcurrentlyDeletingStorage::new(StorageOperation::Delete);
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["old"], "old content").unwrap();
        clock.advance_by(TimeDelta::days(2));

        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        assert_no_cache_entry(&cache, &["old"]);
    }

    #[test]
    fn test_clean_tolerates_meta_files_deleted_concurrently() {
        let mut clock = ControlledClock::new(Utc::now());
        let storage = ConcurrentlyDeletingStorage::new(StorageOperation::Get);
        let mut cache = LocalCache::with_clock(storage, clock.clone());

        cache_entry_with_content(&mut cache, &["key"], "content").unwrap();
        clock.advance_by(TimeDelta::days(2));

        cache.clean(Some(TimeDelta::days(1)), None).unwrap();

        let storage = cache.into_storage();
        assert_blob_count(&storage, 0);
    }

    #[derive(PartialEq)]
    enum StorageOperation {
        Delete,
        Get,
    }

    /// Simulates another process deleting a file right before `operation` accesses it.
    struct ConcurrentlyDeletingStorage {
        storage: InMemoryStorage,
        operation: StorageOperation,
    }

    impl ConcurrentlyDeletingStorage {
        fn new(operation: StorageOperation) -> Self {
            Self {
                storage: InMemoryStorage::new(),
                operation,
            }
        }

        fn delete_before(&self, operation: StorageOperation, path: &str) -> IoPathResult<()> {
            if self.operation == operation {
                self.storage.delete(path)?;
            }
            Ok(())
        }
    }

    impl Storage for ConcurrentlyDeletingStorage {
        type Reader = <InMemoryStorage as Storage>::Reader;
        type Writer = <InMemoryStorage as Storage>::Writer;

        fn delete(&self, path: &str) -> IoPathResult<()> {
            self.delete_before(StorageOperation::Delete, path)?;
            self.storage.delete(path)
        }

        fn exists_file(&self, path: &str) -> IoPathResult<bool> {
            self.storage.exists_file(path)
        }

        fn get(&self, path: &str) -> IoPathResult<FileHandle<Self::Reader>> {
            self.delete_before(StorageOperation::Get, path)?;
            self.storage.get(path)
        }

        fn list(
            &self,
            path: &str,
        ) -> IoPathResult<impl Iterator<Item = IoPathResult<StorageEntry<'_>>>> {
            self.storage.list(path)
        }

        fn put(&self, path: &str) -> IoPathResult<Self::Writer> {
            self.storage.put(path)
        }
    }

    #[test]
    fn test_key_without_blob_is_handled_gracefully() {
        let storage = InMemoryStorage::new();
        let mut cache = LocalCache::new(storage);
        cache_entry_with_content(&mut cache, &["key0"], "cached content").unwrap();

        let storage = cache.into_storage();
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
        assert_cache_entry_with_content(&cache, &["key0", "key1"], "key1", "fallback");
    }

    fn cache_entry_with_content<C: Cache>(
        cache: &mut C,
        keys: &[&str],
        content: &str,
    ) -> IoPathResult<()> {
        let mut writer = cache.set(keys)?;
        let pseudo_path = keys.join(", ");
        writer
            .write_all(content.as_bytes())
            .with_path(&pseudo_path)?;
        writer.close().with_path(&pseudo_path)
    }

    fn assert_cache_entry_with_content<C: Cache>(
        cache: &C,
        keys: &[&str],
        matched_key: &str,
        content: &str,
    ) {
        let CacheHit {
            key,
            mut reader,
            size_hint,
        } = cache
            .get(keys)
            .expect("IO failure getting cache entry")
            .expect("cache entry not found");
        assert_eq!(
            key, matched_key,
            "expected cache key '{}' to be restored, not '{}'",
            matched_key, key
        );
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .expect("failed to read cache entry");
        assert_eq!(buf, content, "cache entry content mismatch");
        assert_eq!(size_hint, Some(content.len() as u64));
    }

    fn assert_no_cache_entry<C: Cache>(cache: &C, keys: &[&str]) {
        let result = cache.get(keys).expect("IO failure getting cache entry");
        assert!(result.is_none(), "unexpected cache entry found");
    }

    fn assert_blob_count<S: Storage>(storage: &S, count: usize) {
        let blob_dirs = storage
            .list("/blob")
            .unwrap()
            .collect::<IoPathResult<Vec<_>>>()
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
