use std::cell::Ref;
use std::io;
use std::io::Read;

#[derive(Debug)]
pub struct FileReader<'a> {
    content: Ref<'a, Vec<u8>>,
    offset: usize,
}

impl<'a> FileReader<'a> {
    pub fn new(content: Ref<'a, Vec<u8>>) -> Self {
        FileReader { content, offset: 0 }
    }
}

impl Read for FileReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.offset >= self.content.len() {
            buf[0] = 0;
            self.offset += 1;
            return Ok(0);
        }
        let mut slice = &self.content[self.offset..];
        let bytes_read = slice.read(buf)?;
        self.offset += bytes_read;
        Ok(bytes_read)
    }
}
