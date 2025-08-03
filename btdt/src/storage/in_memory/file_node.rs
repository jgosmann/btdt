use std::io;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock};

#[derive(Debug, Default)]
pub struct FileNode {
    content: RwLock<Vec<u8>>,
}

impl FileNode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reader(self: &Arc<FileNode>) -> FileReader {
        FileReader::new(Arc::clone(self))
    }

    pub fn writer(self: &Arc<FileNode>) -> FileWriter {
        FileWriter::new(Arc::clone(self))
    }

    pub fn size(&self) -> usize {
        self.content.read().unwrap().len()
    }
}

#[derive(Debug)]
pub struct FileWriter {
    file_node: Arc<FileNode>,
}

impl FileWriter {
    fn new(file_node: Arc<FileNode>) -> Self {
        FileWriter { file_node }
    }
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file_node.content.write().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file_node.content.write().unwrap().flush()
    }
}

#[derive(Debug)]
pub struct FileReader {
    file_node: Arc<FileNode>,
    offset: usize,
}

impl FileReader {
    fn new(file_node: Arc<FileNode>) -> Self {
        FileReader {
            file_node,
            offset: 0,
        }
    }
}

impl Read for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let content = self.file_node.content.read().unwrap();
        if buf.is_empty() {
            return Ok(0);
        }
        if self.offset >= content.len() {
            buf[0] = 0;
            self.offset += 1;
            return Ok(0);
        }
        let mut slice = &content[self.offset..];
        let bytes_read = slice.read(buf)?;
        self.offset += bytes_read;
        Ok(bytes_read)
    }
}
