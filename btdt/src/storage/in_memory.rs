//! Implementation of the `Storage` trait for an in-memory storage.

mod dir_node;
mod file_node;
mod path_iter;

use super::in_memory::dir_node::{DirNode, Node};
use super::in_memory::path_iter::PathIterExt;
use crate::storage::in_memory::file_node::{FileReader, FileWriter};
use crate::storage::{EntryType, Storage, StorageEntry};
use crate::util::close::SelfClosing;
use std::borrow::Cow;
use std::io;
use std::io::ErrorKind;
use std::sync::RwLock;

/// In-memory storage implementation.
///
/// This implementation is mainly intended for testing purposes. It could also be used as a
/// storage in a permanently running server to avoid hitting the disk. However, performance
/// was not a primary concern of the implementation (but might be important to you, if hitting the
/// disk is not an option for you).
///
/// # Examples
///
/// ```rust
/// # use std::io;
/// use std::io::{Read, Write};
/// use btdt::storage::in_memory::InMemoryStorage;
/// use btdt::storage::Storage;
/// use btdt::util::close::Close;
///
/// # fn main() -> io::Result<()> {
/// let mut storage = InMemoryStorage::new();
/// let mut writer = storage.put("/foo/bar")?;
/// writer.write_all(b"Hello, world!")?;
/// writer.close()?;
/// let mut buf = String::new();
/// storage.get("/foo/bar")?.read_to_string(&mut buf)?;
/// assert_eq!(buf, "Hello, world!");
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct InMemoryStorage {
    root: RwLock<DirNode>,
}

impl InMemoryStorage {
    /// Creates a new in-memory storage.
    pub fn new() -> Self {
        InMemoryStorage {
            root: RwLock::new(DirNode::new()),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for InMemoryStorage {
    type Reader = FileReader;
    type Writer = SelfClosing<FileWriter>;

    fn delete(&self, path: &str) -> io::Result<()> {
        let mut dir = &mut *self.root.write().unwrap();
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
                    ));
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Must not delete root directory",
        ))
    }

    fn exists_file(&self, path: &str) -> io::Result<bool> {
        match self.get(path) {
            Ok(_) => Ok(true),
            Err(err) => match err.kind() {
                ErrorKind::IsADirectory | ErrorKind::NotFound => Ok(false),
                _ => Err(err),
            },
        }
    }

    fn get(&self, path: &str) -> io::Result<Self::Reader> {
        let mut dir = &*self.root.read().unwrap();
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
                    ));
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Path must contain at least one component",
        ))
    }

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry<'_>>>> {
        let mut dir = &*self.root.read().unwrap();
        for component in path.path_components()? {
            dir = match dir.get(component.name) {
                Some(Node::Dir(dir)) => dir,
                child => {
                    if component.is_last
                        && let Some(Node::File(_)) = child
                    {
                        return Err(io::Error::new(ErrorKind::NotADirectory, "Not a directory"));
                    }
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ));
                }
            };
        }

        Ok(dir
            .list()
            .map(|(name, node)| {
                Ok(StorageEntry {
                    entry_type: match node {
                        Node::Dir(_) => EntryType::Directory,
                        Node::File(_) => EntryType::File,
                    },
                    name: Cow::Owned(name.clone()),
                    size: match node {
                        Node::Dir(dir) => dir.size() as u64,
                        Node::File(file) => file.size() as u64,
                    },
                })
            })
            .collect::<Vec<_>>()
            .into_iter())
    }

    fn put(&self, path: &str) -> io::Result<Self::Writer> {
        let mut dir = &mut *self.root.write().unwrap();
        let mut components = path.path_components()?;
        for component in components.by_ref() {
            if component.is_last {
                return dir
                    .create_file(component.name.to_string())
                    .map(SelfClosing::new);
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
    use crate::test_storage;

    test_storage!(in_memory_tests, InMemoryStorage::new());
}
