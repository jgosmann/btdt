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

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for InMemoryStorage {
    type Reader = FileReader;
    type Writer = SelfClosing<FileWriter>;

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

    fn exists_file(&mut self, path: &str) -> io::Result<bool> {
        match self.get(path) {
            Ok(_) => Ok(true),
            Err(err) => match err.kind() {
                ErrorKind::IsADirectory | ErrorKind::NotFound => Ok(false),
                _ => Err(err),
            },
        }
    }

    fn get(&self, path: &str) -> io::Result<Self::Reader> {
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

    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>> {
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

        Ok(dir.list().map(|(name, node)| {
            Ok(StorageEntry {
                entry_type: match node {
                    Node::Dir(_) => EntryType::Directory,
                    Node::File(_) => EntryType::File,
                },
                name: Cow::Borrowed(name),
            })
        }))
    }

    fn put(&mut self, path: &str) -> io::Result<Self::Writer> {
        let mut dir = &mut self.root;
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
