mod staged_file;

use crate::storage::filesystem::staged_file::StagedFile;
use crate::storage::{EntryType, Storage, StorageEntry};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::borrow::Cow;
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Component, PathBuf};
use std::{fs, io};

pub struct FilesystemStorage {
    root: PathBuf,
    rng: StdRng,
}

impl FilesystemStorage {
    pub fn new(root: PathBuf) -> Self {
        FilesystemStorage {
            root,
            rng: StdRng::from_os_rng(),
        }
    }
}

impl Storage for FilesystemStorage {
    type Reader = File;
    type Writer = StagedFile<PathBuf>;

    fn delete(&mut self, path: &str) -> io::Result<()> {
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

    fn exists_file(&mut self, path: &str) -> io::Result<bool> {
        Ok(self.canonical_path(path)?.is_file())
    }

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
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

    fn put(&mut self, path: &str) -> io::Result<Self::Writer> {
        let canonical_path = self.canonical_path(path)?;
        if self.root.exists() {
            if let Some(parent_dir) = canonical_path.parent() {
                let mut path = self.root.clone();
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
        }
        StagedFile::new(canonical_path, &mut self.rng)
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
    use crate::storage::tests::write_file_to_storage;
    use crate::test_storage;
    use tempfile::{tempdir, TempDir};

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

        fn delete(&mut self, path: &str) -> io::Result<()> {
            self.storage.delete(path)
        }

        fn get(&self, path: &str) -> io::Result<Self::Reader> {
            self.storage.get(path)
        }

        fn exists_file(&mut self, path: &str) -> io::Result<bool> {
            self.storage.exists_file(path)
        }

        fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
            self.storage.list(path)
        }

        fn put(&mut self, path: &str) -> io::Result<Self::Writer> {
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
}
