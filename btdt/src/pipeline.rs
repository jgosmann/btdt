//! A pipeline defines how multiple files a processed to be stored in the cache, e.g. by archiving
//! them in TAR format and potentially compressing them.

use crate::cache::Cache;
use crate::error::{IoPathResult, WithPath};
use crate::util::close::Close;
use std::io::BufWriter;
use std::path::Path;

/// A pipeline defines how multiple files a processed to be stored in the cache.
///
/// # Examples
///
/// ```rust
/// # use std::fs;
/// # use std::io;
/// use btdt::cache::local::LocalCache;
/// # use btdt::error::IoPathResult;
/// use btdt::pipeline::Pipeline;
/// use btdt::storage::in_memory::InMemoryStorage;
///
/// # fn main() -> IoPathResult<()> {
/// # const CACHEABLE_PATH: &str = "/tmp/btdt-cacheable";
/// # struct CacheableDir;
/// # impl CacheableDir {
/// #     pub fn new() -> Self {
/// #         fs::create_dir(CACHEABLE_PATH).expect(format!("Failed to create directory at {}", CACHEABLE_PATH).as_str());
/// #         Self
/// #     }
/// # }
/// # impl Drop for CacheableDir {
/// #    fn drop(&mut self) {
/// #        fs::remove_dir_all(CACHEABLE_PATH).expect(format!("Failed to remove directory at {}", CACHEABLE_PATH).as_str());
/// #    }
/// # }
/// # let _cacheable_dir = CacheableDir::new();
/// let mut pipeline = Pipeline::new(LocalCache::new(InMemoryStorage::new()));
/// pipeline.store(&["cache-key"], CACHEABLE_PATH)?;
/// pipeline.restore(&["cache-key"], CACHEABLE_PATH)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Pipeline<C: Cache> {
    cache: C,
}

impl<C: Cache> Pipeline<C> {
    /// Creates a new pipeline with the given cache.
    pub fn new(cache: C) -> Self {
        Pipeline { cache }
    }

    /// Restores the files stored in the cache.
    ///
    /// The first key found in the cache is used to restore the files. If no key is found, nothing
    /// is restored. Restored files are written into the directory specified by `destination`.
    ///
    /// Returns `Ok(Some(key))` if files were restored where `key` is the cache key used, `Ok(None)`
    /// otherwise.
    pub fn restore<'a>(
        &self,
        keys: &[&'a str],
        destination: impl AsRef<Path>,
    ) -> IoPathResult<Option<&'a str>> {
        if let Some(cache_hit) = self.cache.get(keys)? {
            tar::Archive::new(cache_hit.reader)
                .unpack(destination.as_ref())
                .with_path(destination.as_ref())?;
            Ok(Some(cache_hit.key))
        } else {
            Ok(None)
        }
    }

    /// Stores the files in the cache.
    ///
    /// The files in the directory specified by `source` are archived and stored in the cache under
    /// the given keys.
    pub fn store(&mut self, keys: &[&str], source: impl AsRef<Path>) -> IoPathResult<()> {
        let mut writer = BufWriter::new(self.cache.set(keys)?);
        {
            let mut archive = tar::Builder::new(&mut writer);
            archive.follow_symlinks(false);
            archive
                .append_dir_all(".", source.as_ref())
                .with_path(source.as_ref())?;
            archive.finish().with_path(source.as_ref())?;
        }
        writer
            .into_inner()
            .map_err(|e| e.into())
            .and_then(Close::close)
            .with_path(source.as_ref())?;
        Ok(())
    }

    /// Consumes the pipeline and returns the cache.
    pub fn into_cache(self) -> C {
        self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::local::LocalCache;
    use crate::storage::in_memory::InMemoryStorage;
    use crate::test_util::fs_spec::{DirSpec, Node};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_roundtrip() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec::create_unix_fixture();

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();
        pipeline.store(&["cache-key"], &source_path).unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(spec.compare_with(&destination_path).unwrap(), vec![]);
    }

    #[test]
    fn test_restore_returns_restored_cache_key() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        fs::create_dir(&source_path).unwrap();
        pipeline.store(&["cache-key-0"], tempdir.path()).unwrap();
        pipeline.store(&["cache-key-1"], tempdir.path()).unwrap();

        let destination_path = tempdir.path().join("destination-root");

        assert!(
            pipeline
                .restore(&["non-existent"], &destination_path)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            pipeline
                .restore(
                    &["non-existent", "cache-key-1", "cache-key-0"],
                    &destination_path
                )
                .unwrap(),
            Some("cache-key-1")
        );
    }
}
