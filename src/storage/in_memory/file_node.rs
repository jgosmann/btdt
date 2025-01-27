use std::cell::RefCell;
use std::io;
use std::io::{Read, Write};
use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub struct FileNode {
    content: RefCell<Vec<u8>>,
}

impl FileNode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reader(self: &Rc<FileNode>) -> FileReader {
        FileReader::new(Rc::clone(self))
    }

    pub fn writer(self: &Rc<FileNode>) -> FileWriter {
        FileWriter::new(Rc::clone(self))
    }
}

#[derive(Debug)]
pub struct FileWriter {
    file_node: Rc<FileNode>,
}

impl FileWriter {
    fn new(file_node: Rc<FileNode>) -> Self {
        FileWriter { file_node }
    }
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file_node.content.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file_node.content.borrow_mut().flush()
    }
}

#[derive(Debug)]
pub struct FileReader {
    file_node: Rc<FileNode>,
    offset: usize,
}

impl FileReader {
    fn new(file_node: Rc<FileNode>) -> Self {
        FileReader {
            file_node,
            offset: 0,
        }
    }
}

impl Read for FileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let content = self.file_node.content.borrow();
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
