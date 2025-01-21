mod dir_node;
mod file_node;
mod file_reader;
mod path_iter;

use super::in_memory::dir_node::{DirNode, Node};
use super::in_memory::path_iter::PathIterExt;
use crate::storage::{EntryType, Storage, StorageEntry};
use std::borrow::Cow;
use std::io;
use std::io::{ErrorKind, Read, Write};

#[derive(Debug, Clone)]
pub struct InMemoryStorage {
    root: DirNode,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage {
            root: DirNode::new(),
        }
    }
}

impl Storage for InMemoryStorage {
    fn delete(&mut self, path: &str) -> io::Result<()> {
        let mut dir = &mut self.root;
        for component in path.path_components()? {
            if component.is_last {
                return dir.delete(component.name);
            }

            dir = match dir.get_mut(component.name) {
                Some(Node::Dir(dir)) => dir,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ))
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Must not delete root directory",
        ))
    }

    fn get(&self, path: &str) -> io::Result<impl Read> {
        let mut dir = &self.root;
        let mut components = path.path_components()?;
        for component in components.by_ref() {
            if component.is_last {
                return match dir.get(component.name) {
                    Some(Node::File(file)) => Ok(file.reader()),
                    Some(Node::Dir(_)) => {
                        Err(io::Error::new(ErrorKind::IsADirectory, "Is a directory"))
                    }
                    _ => Err(io::Error::new(ErrorKind::NotFound, "File not found")),
                };
            }

            dir = match dir.get(component.name) {
                Some(Node::Dir(dir)) => dir,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ))
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Path must contain at least one component",
        ))
    }

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = StorageEntry>> {
        let mut dir = &self.root;
        for component in path.path_components()? {
            dir = match dir.get(component.name) {
                Some(Node::Dir(dir)) => dir,
                child => {
                    if component.is_last {
                        if let Some(Node::File(_)) = child {
                            return Err(io::Error::new(
                                ErrorKind::NotADirectory,
                                "Not a directory",
                            ));
                        }
                    }
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ));
                }
            };
        }

        Ok(dir.list().map(|(name, node)| StorageEntry {
            entry_type: match node {
                Node::Dir(_) => EntryType::Directory,
                Node::File(_) => EntryType::File,
            },
            name: Cow::Borrowed(name),
        }))
    }

    fn put(&mut self, path: &str) -> io::Result<impl Write> {
        let mut dir = &mut self.root;
        let mut components = path.path_components()?;
        for component in components.by_ref() {
            if component.is_last {
                return dir.create_file(component.name.to_string());
            }

            dir = dir.get_or_insert_dir(component.name.to_string())?;
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Path must contain at least one component",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_returns_error_for_non_existent_file() {
        let storage = InMemoryStorage::new();
        let result = storage.get("/non-existent-file.txt");
        assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_list_returns_error_for_non_existent_dir() {
        let storage = InMemoryStorage::new();
        let result = storage.list("/non-existent-dir");
        assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_list_returns_error_for_non_existent_file_or_dir() {
        let mut storage = InMemoryStorage::new();
        let result = storage.delete("/non-existent");
        assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_can_get_file_that_was_previously_put_into_storage() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/dir/file.txt", "Hello, world!").unwrap();
        assert_eq!(
            &read_file_from_storage_to_string(&storage, "/dir/file.txt").unwrap(),
            "Hello, world!"
        );
    }

    #[test]
    fn test_different_files_are_separate() {
        let mut storage = InMemoryStorage::new();

        write_file_to_storage(&mut storage, "/a.txt", "Hello, a!").unwrap();
        write_file_to_storage(&mut storage, "/b.txt", "Hello, b!").unwrap();

        assert_eq!(
            &read_file_from_storage_to_string(&storage, "/a.txt").unwrap(),
            "Hello, a!"
        );
        assert_eq!(
            &read_file_from_storage_to_string(&storage, "/b.txt").unwrap(),
            "Hello, b!"
        );
    }

    #[test]
    fn test_can_overwrite_existing_file() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/file.txt", "Hello, world!").unwrap();
        write_file_to_storage(&mut storage, "/file.txt", "Bye, world!").unwrap();
        assert_eq!(
            &read_file_from_storage_to_string(&storage, "/file.txt").unwrap(),
            "Bye, world!"
        );
    }

    #[test]
    fn test_errors_when_trying_to_overwrite_dir_with_file() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/dir/file.txt", "file-content").unwrap();
        assert!(storage.put("dir").is_err());
    }

    #[test]
    fn test_list_returns_direct_children_of_directory() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/rootfile.txt", "rootfile-content").unwrap();
        write_file_to_storage(&mut storage, "/dir/file1.txt", "file1-content").unwrap();
        write_file_to_storage(&mut storage, "/dir/file2.txt", "file2-content").unwrap();
        write_file_to_storage(&mut storage, "/dir/subdir/subfile.txt", "subfile-content").unwrap();

        let mut entries: Vec<_> = storage.list("/").unwrap().collect();
        entries.sort_unstable_by_key(|entry| entry.name.to_string());
        assert_eq!(
            entries,
            vec![
                StorageEntry {
                    entry_type: EntryType::Directory,
                    name: Cow::Owned("dir".to_string()),
                },
                StorageEntry {
                    entry_type: EntryType::File,
                    name: Cow::Owned("rootfile.txt".to_string()),
                }
            ]
        );

        let mut entries: Vec<_> = storage.list("/dir").unwrap().collect();
        entries.sort_unstable_by_key(|entry| entry.name.to_string());
        assert_eq!(
            entries,
            vec![
                StorageEntry {
                    entry_type: EntryType::File,
                    name: Cow::Owned("file1.txt".to_string()),
                },
                StorageEntry {
                    entry_type: EntryType::File,
                    name: Cow::Owned("file2.txt".to_string()),
                },
                StorageEntry {
                    entry_type: EntryType::Directory,
                    name: Cow::Owned("subdir".to_string()),
                },
            ]
        );
    }

    #[test]
    fn test_can_delete_file() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/file.txt", "file-content").unwrap();
        storage.delete("/file.txt").unwrap();
        assert_eq!(
            storage.get("/file.txt").err().unwrap().kind(),
            ErrorKind::NotFound
        );
        assert_eq!(storage.list("/").unwrap().count(), 0);
    }

    #[test]
    fn test_can_delete_empty_directory() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/dir/file.txt", "file-content").unwrap();
        storage.delete("/dir/file.txt").unwrap();
        storage.delete("/dir").unwrap();
        assert_eq!(storage.list("/").unwrap().count(), 0);
    }

    #[test]
    fn test_delete_returns_error_for_non_empty_dir() {
        let mut storage = InMemoryStorage::new();
        write_file_to_storage(&mut storage, "/dir/file.txt", "file-content").unwrap();
        assert_eq!(
            storage.delete("/dir").err().unwrap().kind(),
            ErrorKind::DirectoryNotEmpty
        );
    }

    fn write_file_to_storage(
        storage: &mut InMemoryStorage,
        path: &str,
        content: &str,
    ) -> io::Result<()> {
        let mut writer = storage.put(path)?;
        writer.write_all(content.as_bytes())
    }

    fn read_file_from_storage_to_string(
        storage: &InMemoryStorage,
        path: &str,
    ) -> io::Result<String> {
        let mut reader = storage.get(path)?;
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Ok(buf)
    }
}
