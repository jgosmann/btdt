use super::file_reader::FileReader;
use std::cell::RefCell;
use std::io;
use std::io::Write;

#[derive(Debug, Clone, Default)]
pub struct FileNode {
    content: RefCell<Vec<u8>>,
}

impl FileNode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reader(&self) -> FileReader {
        FileReader::new(self.content.borrow())
    }
}

impl Write for &FileNode {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.content.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.content.borrow_mut().flush()
    }
}
