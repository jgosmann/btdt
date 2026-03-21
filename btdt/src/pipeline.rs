//! A pipeline defines how multiple files a processed to be stored in the cache, e.g. by archiving
//! them in TAR format and potentially compressing them.

use crate::cache::Cache;
use crate::error::{IoPathError, IoPathResult, WithPath};
use crate::util::close::Close;
use ignore::overrides::Override;
use ignore::{Error, WalkBuilder};
use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};
use std::path::Path;
use tar::{Builder, EntryType, Header};

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
    ///
    /// Files named `.btdtignore` can be used to exclude files from the cache. The syntax follows
    /// the [`.gitignore` specification](https://git-scm.com/docs/gitignore).
    pub fn store(&mut self, keys: &[&str], source: impl AsRef<Path>) -> IoPathResult<()> {
        self.store_with_overrides(keys, source, Override::empty())
    }

    /// Stores the files in the cache.
    ///
    /// The files in the directory specified by `source` are archived and stored in the cache under
    /// the given keys.
    ///
    /// Files named `.btdtignore` can be used to exclude files from the cache. The syntax follows
    /// the [`.gitignore` specification](https://git-scm.com/docs/gitignore).
    pub fn store_with_overrides(
        &mut self,
        keys: &[&str],
        source: impl AsRef<Path>,
        overrides: Override,
    ) -> IoPathResult<()> {
        let mut writer = BufWriter::new(self.cache.set(keys)?);
        {
            let mut archive = tar::Builder::new(&mut writer);
            archive.follow_symlinks(false);
            Self::add_dir_to_archive(&mut archive, source.as_ref(), overrides)?;
            archive.finish().with_path(source.as_ref())?;
        }
        writer
            .into_inner()
            .map_err(|e| e.into())
            .and_then(Close::close)
            .with_path(source.as_ref())?;
        Ok(())
    }

    fn add_dir_to_archive(
        archive_builder: &mut Builder<impl Write>,
        root: &Path,
        overrides: Override,
    ) -> IoPathResult<()> {
        let walker = WalkBuilder::new(root)
            .follow_links(false)
            .standard_filters(false)
            .add_custom_ignore_filename(".btdtignore")
            .overrides(overrides)
            .build();

        for entry in walker {
            let entry = entry.map_err(|err| match err {
                Error::WithPath { path, err } => IoPathError::new(io::Error::other(err), path),
                err => IoPathError::new_no_path(io::Error::other(err)),
            })?;

            let source_path = entry.path();
            if source_path == root {
                continue;
            }
            let archived_path = source_path
                .strip_prefix(root)
                .expect("root not a prefix of parth");

            let file_type = entry.file_type().expect("file type should be available");
            if file_type.is_dir() {
                archive_builder
                    .append_dir(archived_path, source_path)
                    .with_path(source_path)?;
            } else if file_type.is_symlink() {
                let link_target = std::fs::read_link(source_path).with_path(source_path)?;
                let mut header = Header::new_old();
                header.set_entry_type(EntryType::Symlink);
                header.set_size(0);
                archive_builder
                    .append_link(&mut header, archived_path, link_target)
                    .with_path(source_path)?;
            } else if file_type.is_file() {
                let mut file = File::open(source_path).with_path(source_path)?;
                archive_builder
                    .append_file(archived_path, &mut file)
                    .with_path(entry.path())?;
            } else {
                return Err(io::Error::other("Unsupported file type")).with_path(entry.path());
            }
        }
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
    use crate::test_util::fs_spec::{DirSpec, FileSpec, Node};
    use ignore::overrides::OverrideBuilder;
    use std::fs;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn file_with_name(name: &str) -> (String, Box<dyn Node>) {
        (
            name.to_string(),
            Box::new(FileSpec {
                permissions: Permissions::from_mode(0o644),
                content: vec![],
            }) as Box<dyn Node>,
        )
    }

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
    fn test_respects_btdtignore_files() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec {
            permissions: Permissions::from_mode(0o755),
            children: [
                (
                    ".btdtignore".to_string(),
                    Box::new(FileSpec {
                        permissions: Permissions::from_mode(0o644),
                        content: b"
# comment
/ignore-root-only
ignore-everywhere
ignore-only-dir/
ignore-*-wildcard
subpath/**/ignore
!**/include
                "
                        .to_vec(),
                    }) as Box<dyn Node>,
                ),
                file_with_name("some-file"),
                file_with_name(".hidden-file"),
                file_with_name("include"),
                file_with_name("ignore-root-only"),
                file_with_name("ignore-everywhere"),
                file_with_name("ignore-foo-wildcard"),
                file_with_name("ignore-with-local-ignore-file"),
                (
                    "ignore-only-dir".to_string(),
                    Box::new(DirSpec {
                        permissions: Permissions::from_mode(0o755),
                        children: [file_with_name("foo"), file_with_name("include")]
                            .into_iter()
                            .collect(),
                    }) as Box<dyn Node>,
                ),
                (
                    "subpath".to_string(),
                    Box::new(DirSpec {
                        permissions: Permissions::from_mode(0o755),
                        children: [
                            file_with_name("ignore-root-only"),
                            file_with_name("ignore-everywhere"),
                            file_with_name("ignore-only-dir"),
                            file_with_name("ignore-with-local-ignore-file"),
                            (
                                ".btdtignore".to_string(),
                                Box::new(FileSpec {
                                    permissions: Permissions::from_mode(0o644),
                                    content: b"ignore-with-local-ignore-file".to_vec(),
                                }) as Box<dyn Node>,
                            ),
                            (
                                "subsubpath".to_string(),
                                Box::new(DirSpec {
                                    permissions: Permissions::from_mode(0o755),
                                    children: [file_with_name("foo"), file_with_name("ignore")]
                                        .into_iter()
                                        .collect(),
                                }) as Box<dyn Node>,
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    }) as Box<dyn Node>,
                ),
            ]
            .into_iter()
            .collect(),
        };
        let expected = DirSpec {
            permissions: Permissions::from_mode(0o755),
            children: [
                (
                    ".btdtignore".to_string(),
                    Box::new(FileSpec {
                        permissions: Permissions::from_mode(0o644),
                        content: b"
# comment
/ignore-root-only
ignore-everywhere
ignore-only-dir/
ignore-*-wildcard
subpath/**/ignore
!**/include
                "
                        .to_vec(),
                    }) as Box<dyn Node>,
                ),
                file_with_name("some-file"),
                file_with_name(".hidden-file"),
                file_with_name("include"),
                file_with_name("ignore-with-local-ignore-file"),
                (
                    "subpath".to_string(),
                    Box::new(DirSpec {
                        permissions: Permissions::from_mode(0o755),
                        children: [
                            file_with_name("ignore-root-only"),
                            file_with_name("ignore-only-dir"),
                            (
                                ".btdtignore".to_string(),
                                Box::new(FileSpec {
                                    permissions: Permissions::from_mode(0o644),
                                    content: b"ignore-with-local-ignore-file".to_vec(),
                                }) as Box<dyn Node>,
                            ),
                            (
                                "subsubpath".to_string(),
                                Box::new(DirSpec {
                                    permissions: Permissions::from_mode(0o755),
                                    children: [file_with_name("foo")].into_iter().collect(),
                                }) as Box<dyn Node>,
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    }) as Box<dyn Node>,
                ),
            ]
            .into_iter()
            .collect(),
        };

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();
        pipeline.store(&["cache-key"], &source_path).unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(expected.compare_with(&destination_path).unwrap(), vec![]);
    }

    #[test]
    fn test_does_not_use_gitignore_files() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec {
            permissions: Permissions::from_mode(0o755),
            children: [
                (
                    ".gitignore".to_string(),
                    Box::new(FileSpec {
                        permissions: Permissions::from_mode(0o644),
                        content: b"do-not-ignore".to_vec(),
                    }) as Box<dyn Node>,
                ),
                (
                    "do-not-ignore".to_string(),
                    Box::new(FileSpec {
                        permissions: Permissions::from_mode(0o644),
                        content: vec![],
                    }) as Box<dyn Node>,
                ),
            ]
            .into_iter()
            .collect(),
        };

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();
        pipeline.store(&["cache-key"], &source_path).unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(spec.compare_with(&destination_path).unwrap(), vec![]);
    }

    #[test]
    fn test_store_with_overrides() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec {
            permissions: Permissions::from_mode(0o755),
            children: [file_with_name("some-file"), file_with_name("ignore")]
                .into_iter()
                .collect(),
        };
        let expected = DirSpec {
            permissions: Permissions::from_mode(0o755),
            children: [file_with_name("some-file")].into_iter().collect(),
        };

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();

        let overrides = OverrideBuilder::new(&source_path)
            .add("!ignore")
            .unwrap()
            .build()
            .unwrap();
        pipeline
            .store_with_overrides(&["cache-key"], &source_path, overrides)
            .unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(expected.compare_with(&destination_path).unwrap(), vec![]);
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
