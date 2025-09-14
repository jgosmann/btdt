use super::file_node::{FileNode, FileWriter};
use crate::error::{IoPathError, IoPathResult};
use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Node {
    Dir(DirNode),
    File(Arc<FileNode>),
}

#[derive(Debug, Clone)]
pub struct DirNode(HashMap<String, Node>);

impl DirNode {
    pub fn new() -> Self {
        DirNode(HashMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn list(&self) -> impl Iterator<Item = (&String, &Node)> {
        self.0.iter()
    }

    pub fn get(&self, name: &str) -> Option<&Node> {
        self.0.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.0.get_mut(name)
    }

    pub fn get_or_insert_dir(&mut self, name: &str) -> IoPathResult<&mut DirNode> {
        match self
            .0
            .entry(name.to_string())
            .or_insert(Node::Dir(DirNode::new()))
        {
            Node::Dir(dir) => Ok(dir),
            _ => Err(IoPathError::new(
                io::Error::new(ErrorKind::NotADirectory, "Not a directory"),
                name,
            )),
        }
    }

    pub fn delete(&mut self, name: &str) -> IoPathResult<()> {
        if let Some(node) = self.0.get(name) {
            if let Node::Dir(dir) = node
                && !dir.is_empty()
            {
                return Err(IoPathError::new(
                    io::Error::new(
                        ErrorKind::DirectoryNotEmpty,
                        "Directory must be empty to be deleted",
                    ),
                    name,
                ));
            }
            self.0.remove(name);
            Ok(())
        } else {
            Err(IoPathError::new(
                io::Error::new(ErrorKind::NotFound, "No such file or directory"),
                name,
            ))
        }
    }

    pub fn create_file(&mut self, name: &str) -> IoPathResult<FileWriter> {
        let node = self
            .0
            .entry(name.to_string())
            .and_modify(|node| {
                if let Node::File(_) = node {
                    *node = Node::File(Arc::new(FileNode::new()));
                }
            })
            .or_insert(Node::File(Arc::new(FileNode::new())));
        match node {
            Node::File(file) => Ok(file.writer()),
            Node::Dir(_) => Err(IoPathError::new(
                io::Error::new(
                    ErrorKind::IsADirectory,
                    "A directory with the same name already exists",
                ),
                name,
            )),
        }
    }

    pub fn size(&self) -> usize {
        0
    }
}
