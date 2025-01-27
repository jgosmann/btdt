mod staged_file;

use crate::close::Close;
use crate::storage::filesystem::staged_file::StagedFile;
use crate::storage::{EntryType, Storage, StorageEntry};
use std::borrow::Cow;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, PathBuf};
use std::{fs, io};

pub struct FilesystemStorage {
    root: PathBuf,
}

impl FilesystemStorage {
    pub fn new(root: PathBuf) -> Self {
        FilesystemStorage { root }
    }
}

impl Storage for FilesystemStorage {
    type Reader<'a> = File;
    type Writer<'a> = StagedFile<PathBuf>;

    fn delete(&mut self, path: &str) -> io::Result<()> {
        let full_path = self.root.join(self.canonical_path(path)?);
        if full_path.is_dir() {
            fs::remove_dir(full_path)
        } else {
            fs::remove_file(full_path)
        }
    }

    fn get<'a>(&'a self, path: &str) -> io::Result<Self::Reader<'a>> {
        File::open(self.root.join(self.canonical_path(path)?))
    }

    fn exists_file(&mut self, path: &str) -> io::Result<bool> {
        Ok(self.root.join(self.canonical_path(path)?).is_file())
    }

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
        Ok(self
            .root
            .join(self.canonical_path(path)?)
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
                    }))
                } else {
                    Ok(None)
                }
            })
            .filter_map(Result::transpose))
    }

    fn put<'a>(&'a mut self, path: &str) -> io::Result<Self::Writer<'a>> {
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
        StagedFile::new(self.root.join(canonical_path))
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
        type Reader<'a> = <FilesystemStorage as Storage>::Reader<'a>;
        type Writer<'a> = <FilesystemStorage as Storage>::Writer<'a>;

        fn delete(&mut self, path: &str) -> io::Result<()> {
            self.storage.delete(path)
        }

        fn get<'a>(&'a self, path: &str) -> io::Result<Self::Reader<'a>> {
            self.storage.get(path)
        }

        fn exists_file(&mut self, path: &str) -> io::Result<bool> {
            self.storage.exists_file(path)
        }

        fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
            self.storage.list(path)
        }

        fn put<'a>(&'a mut self, path: &str) -> io::Result<Self::Writer<'a>> {
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
