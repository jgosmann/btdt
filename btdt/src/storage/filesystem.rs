//! Implementation of the `Storage` trait for storage in the local filesystem.

mod staged_file;

use crate::storage::filesystem::staged_file::{StagedFile, clean_leftover_tmp_files};
use crate::storage::{EntryType, Storage, StorageEntry};
use rand::rngs::ThreadRng;
use std::borrow::Cow;
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Component, PathBuf};
use std::{fs, io};

/// Storage implementation using the local filesystem.
///
/// Multiple instances of this storage with the same root directory may be used in parallel.
///
/// # Examples
///
/// ```rust
/// # use std::io;
/// # use std::fs;
/// use std::io::{Read, Write};
/// use btdt::storage::filesystem::FilesystemStorage;
/// use btdt::storage::Storage;
/// use btdt::util::close::Close;
///
/// # const STORAGE_PATH: &str = "/tmp/btdt-storage";
/// # struct StorageDir;
/// # impl StorageDir {
/// #     pub fn new() -> Self {
/// #         fs::create_dir(STORAGE_PATH).expect(format!("Failed to create storage directory at {}", STORAGE_PATH).as_str());
/// #         Self
/// #     }
/// # }
/// # impl Drop for StorageDir {
/// #    fn drop(&mut self) {
/// #        fs::remove_dir_all(STORAGE_PATH).expect(format!("Failed to remove storage directory at {}", STORAGE_PATH).as_str());
/// #    }
/// # }
///
/// # fn main() -> io::Result<()> {
/// # let _storage_dir = StorageDir::new();
/// let mut storage = FilesystemStorage::new(STORAGE_PATH.into());
/// let mut writer = storage.put("/foo/bar")?;
/// writer.write_all(b"Hello, world!")?;
/// writer.close()?;
/// let mut buf = String::new();
/// storage.get("/foo/bar")?.read_to_string(&mut buf)?;
/// assert_eq!(buf, "Hello, world!");
/// # Ok(())
/// # }
/// ```
pub struct FilesystemStorage {
    root: PathBuf,
}

impl FilesystemStorage {
    /// Creates a new filesystem storage with the given root directory.
    ///
    /// All paths will be nested in the given root directory.
    pub fn new(root: PathBuf) -> Self {
        FilesystemStorage { root }
    }

    /// Cleans up leftover temporary files in the storage.
    ///
    /// The filesystem storage writes temporary files to ensure atomic writes. Usually these will
    /// be deleted automatically when the writer is dropped. However, if the process is killed hard,
    /// these files might be left behind. This method can be used to clean them up.
    pub fn clean_leftover_tmp_files(&mut self) -> io::Result<()> {
        clean_leftover_tmp_files(&self.root)
    }
}

impl Storage for FilesystemStorage {
    type Reader = File;
    type Writer = StagedFile<PathBuf>;

    fn delete(&self, path: &str) -> io::Result<()> {
        let full_path = self.canonical_path(path)?;
        if full_path.is_dir() {
            fs::remove_dir(full_path)
        } else {
            fs::remove_file(full_path)
        }
    }

    fn get(&self, path: &str) -> io::Result<Self::Reader> {
        File::open(self.canonical_path(path)?)
    }

    fn exists_file(&self, path: &str) -> io::Result<bool> {
        Ok(self.canonical_path(path)?.is_file())
    }

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry<'_>>>> {
        Ok(self
            .canonical_path(path)?
            .read_dir()?
            .map(|entry| {
                let entry = entry?;
                if let Some(entry_type) = match entry.file_type()? {
                    file_type if file_type.is_file() => Some(EntryType::File),
                    file_type if file_type.is_dir() => Some(EntryType::Directory),
                    _ => None,
                } {
                    Ok(Some(StorageEntry {
                        name: Cow::Owned(entry.file_name().into_string().map_err(|_| {
                            io::Error::new(ErrorKind::InvalidData, "File name is not valid Unicode")
                        })?),
                        entry_type,
                        size: if entry.file_type()?.is_file() {
                            entry.metadata()?.len()
                        } else {
                            0
                        },
                    }))
                } else {
                    Ok(None)
                }
            })
            .filter_map(Result::transpose))
    }

    fn put(&self, path: &str) -> io::Result<Self::Writer> {
        let canonical_path = self.canonical_path(path)?;
        if self.root.exists()
            && let Some(parent_dir) = canonical_path.parent()
        {
            let mut path = PathBuf::new();
            for component in parent_dir.components() {
                if component == Component::ParentDir {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "Path must not contain parent directory components",
                    ));
                }
                path = path.join(component);
                if !path.exists() {
                    fs::create_dir(&path)?;
                }
            }
        }
        StagedFile::new(canonical_path, &mut ThreadRng::default())
    }
}

impl FilesystemStorage {
    fn canonical_path(&self, path: &str) -> io::Result<PathBuf> {
        if !path.starts_with('/') {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Path must be absolute, i.e. start with a slash '/'",
            ));
        }
        Ok(self.root.join(&path[1..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::{read_file_from_storage_to_string, write_file_to_storage};
    use crate::test_storage;
    use std::fs::create_dir_all;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};

    struct FilesystemStorageTestFixture {
        storage: FilesystemStorage,
        _tempdir: TempDir,
    }

    impl FilesystemStorageTestFixture {
        fn new() -> Self {
            let tempdir = tempdir().unwrap();
            Self {
                storage: FilesystemStorage::new(tempdir.path().to_path_buf()),
                _tempdir: tempdir,
            }
        }
    }

    impl Storage for FilesystemStorageTestFixture {
        type Reader = <FilesystemStorage as Storage>::Reader;
        type Writer = <FilesystemStorage as Storage>::Writer;

        fn delete(&self, path: &str) -> io::Result<()> {
            self.storage.delete(path)
        }

        fn get(&self, path: &str) -> io::Result<Self::Reader> {
            self.storage.get(path)
        }

        fn exists_file(&self, path: &str) -> io::Result<bool> {
            self.storage.exists_file(path)
        }

        fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
            self.storage.list(path)
        }

        fn put(&self, path: &str) -> io::Result<Self::Writer> {
            self.storage.put(path)
        }
    }

    test_storage!(filesystem_tests, FilesystemStorageTestFixture::new());

    #[test]
    fn test_does_not_create_non_existent_root() {
        let tempdir = tempdir().unwrap();
        let storage_path = tempdir.path().join("non-existent");
        let mut storage = FilesystemStorage::new(storage_path.clone());
        assert_eq!(
            write_file_to_storage(&mut storage, "/file.txt", "Hello, world!")
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );
        assert!(!storage_path.exists());
    }

    #[test]
    fn test_disallows_putting_files_above_root() {
        let tempdir = tempdir().unwrap();
        let storage_root = tempdir.path().join("storage-root");
        fs::create_dir(&storage_root).unwrap();
        let mut storage = FilesystemStorage::new(storage_root.clone());
        assert!(write_file_to_storage(&mut storage, "/../file.txt", "file-content").is_err());
        assert!(!storage_root.join("file.txt").exists());
    }

    struct PushCwd {
        old_cwd: PathBuf,
    }

    impl PushCwd {
        fn new<P: AsRef<Path>>(new_cwd: P) -> io::Result<Self> {
            let old_cwd = std::env::current_dir()?;
            std::env::set_current_dir(new_cwd)?;
            Ok(Self { old_cwd })
        }
    }

    impl Drop for PushCwd {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.old_cwd).unwrap();
        }
    }

    #[test]
    fn test_with_relative_path() {
        let tempdir = tempdir().unwrap();
        let _push_cwd = PushCwd::new(tempdir.path()).unwrap();
        let storage_path = PathBuf::from("dir/storage-root");
        create_dir_all(&storage_path).unwrap();
        let mut storage = FilesystemStorage::new(storage_path.clone());
        write_file_to_storage(&mut storage, "/some/subdir/file.txt", "Hello, world!").unwrap();
        read_file_from_storage_to_string(&mut storage, "/some/subdir/file.txt").unwrap();
    }
}
