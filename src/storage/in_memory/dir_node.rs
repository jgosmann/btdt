use super::file_node::{FileNode, FileWriter};
use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Node {
    Dir(DirNode),
    File(Rc<FileNode>),
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

    pub fn get_or_insert_dir(&mut self, name: String) -> io::Result<&mut DirNode> {
        match self.0.entry(name).or_insert(Node::Dir(DirNode::new())) {
            Node::Dir(dir) => Ok(dir),
            _ => Err(io::Error::new(ErrorKind::NotADirectory, "Not a directory")),
        }
    }

    pub fn delete(&mut self, name: &str) -> io::Result<()> {
        if let Some(node) = self.0.get(name) {
            if let Node::Dir(dir) = node {
                if !dir.is_empty() {
                    return Err(io::Error::new(
                        ErrorKind::DirectoryNotEmpty,
                        "Directory must be empty to be deleted",
                    ));
                }
            }
            self.0.remove(name);
            Ok(())
        } else {
            Err(io::Error::new(
                ErrorKind::NotFound,
                "No such file or directory",
            ))
        }
    }

    pub fn create_file(&mut self, name: String) -> io::Result<FileWriter> {
        let node = self
            .0
            .entry(name)
            .and_modify(|node| {
                if let Node::File(_) = node {
                    *node = Node::File(Rc::new(FileNode::new()));
                }
            })
            .or_insert(Node::File(Rc::new(FileNode::new())));
        match node {
            Node::File(file) => Ok(file.writer()),
            Node::Dir(_) => Err(io::Error::new(
                ErrorKind::IsADirectory,
                "A directory with the same name already exists",
            )),
        }
    }

    pub fn size(&self) -> usize {
        0
    }
}
