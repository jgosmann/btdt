//! Implementation of the `Storage` trait for an in-memory storage.

mod dir_node;
mod file_node;
mod path_iter;

use super::in_memory::dir_node::{DirNode, Node};
use super::in_memory::path_iter::PathIterExt;
use crate::error::{IoPathResult, WithPath};
use crate::storage::in_memory::file_node::{FileReader, FileWriter};
use crate::storage::{EntryType, FileHandle, Storage, StorageEntry};
use crate::util::close::SelfClosing;
use std::borrow::Cow;
use std::io;
use std::io::ErrorKind;
use std::sync::{Arc, RwLock};

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
/// use btdt::error::WithPath;
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
/// let mut reader = storage.get("/foo/bar")?.reader;
/// reader.read_to_string(&mut buf)?;
/// assert_eq!(buf, "Hello, world!");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct InMemoryStorage {
    root: Arc<RwLock<DirNode>>,
}

impl InMemoryStorage {
    /// Creates a new in-memory storage.
    pub fn new() -> Self {
        InMemoryStorage {
            root: Arc::new(RwLock::new(DirNode::new())),
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

    fn delete(&self, path: &str) -> IoPathResult<()> {
        let mut dir = &mut *self.root.write().unwrap();
        for (i, component) in path.path_components().with_path(path)?.enumerate() {
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
                    .with_path(first_n_path_components(path, i + 1)?);
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Must not delete root directory",
        ))
        .with_path(path)
    }

    fn exists_file(&self, path: &str) -> IoPathResult<bool> {
        match self.get(path) {
            Ok(_) => Ok(true),
            Err(err) => match err.io_error().kind() {
                ErrorKind::IsADirectory | ErrorKind::NotFound => Ok(false),
                _ => Err(err),
            },
        }
    }

    fn get(&self, path: &str) -> IoPathResult<FileHandle<Self::Reader>> {
        let mut dir = &*self.root.read().unwrap();
        let mut components = path.path_components().with_path(path)?;
        for (i, component) in components.by_ref().enumerate() {
            if component.is_last {
                return match dir.get(component.name) {
                    Some(Node::File(file)) => Ok(FileHandle {
                        size_hint: file.size() as u64,
                        reader: file.reader(),
                    }),
                    Some(Node::Dir(_)) => {
                        Err(io::Error::new(ErrorKind::IsADirectory, "Is a directory"))
                            .with_path(first_n_path_components(path, i + 1)?)
                    }
                    _ => Err(io::Error::new(ErrorKind::NotFound, "File not found"))
                        .with_path(first_n_path_components(path, i + 1)?),
                };
            }

            dir = match dir.get(component.name) {
                Some(Node::Dir(dir)) => dir,
                _ => {
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ))
                    .with_path(first_n_path_components(path, i + 1)?);
                }
            };
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Path must contain at least one component",
        ))
        .with_path(path)
    }

    fn list(
        &self,
        path: &str,
    ) -> IoPathResult<impl Iterator<Item = IoPathResult<StorageEntry<'_>>>> {
        let mut dir = &*self.root.read().unwrap();
        for (i, component) in path.path_components().with_path(path)?.enumerate() {
            dir = match dir.get(component.name) {
                Some(Node::Dir(dir)) => dir,
                child => {
                    let err_path = first_n_path_components(path, i + 1)?;
                    if component.is_last
                        && let Some(Node::File(_)) = child
                    {
                        return Err(io::Error::new(ErrorKind::NotADirectory, "Not a directory"))
                            .with_path(err_path);
                    }
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "No such file or directory",
                    ))
                    .with_path(err_path);
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

    fn put(&self, path: &str) -> IoPathResult<Self::Writer> {
        let mut dir = &mut *self.root.write().unwrap();
        let mut components = path.path_components().with_path(path)?;
        for component in components.by_ref() {
            if component.is_last {
                return dir.create_file(component.name).map(SelfClosing::new);
            }

            dir = dir.get_or_insert_dir(component.name)?;
        }

        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Path must contain at least one component",
        ))
        .with_path(path)
    }
}

fn first_n_path_components(path: &str, n: usize) -> IoPathResult<String> {
    let components: Vec<_> = path
        .path_components()
        .with_path(path)?
        .take(n)
        .map(|c| c.name)
        .collect();
    Ok(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::write_file_to_storage;
    use crate::test_storage;

    test_storage!(in_memory_tests, InMemoryStorage::new());

    #[test]
    fn test_provides_size_hint() {
        let storage = InMemoryStorage::new();
        write_file_to_storage(&storage, "/dir/file.txt", "Hello, world!").unwrap();
        assert_eq!(
            storage.get("/dir/file.txt").unwrap().size_hint,
            "Hello, world!".len() as u64
        );
    }
}
