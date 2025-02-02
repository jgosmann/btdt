pub mod filesystem;
pub mod in_memory;
#[cfg(test)]
pub mod tests;

use super::util::close::Close;
use std::borrow::Cow;
use std::io;
use std::io::{Read, Write};

pub trait Storage {
    type Reader: Read;
    type Writer: Write + Close;

    fn delete(&mut self, path: &str) -> io::Result<()>;
    fn exists_file(&mut self, path: &str) -> io::Result<bool>;
    fn get(&self, path: &str) -> io::Result<Self::Reader>;
    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>>;
    fn put(&mut self, path: &str) -> io::Result<Self::Writer>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageEntry<'a> {
    pub entry_type: EntryType,
    pub name: Cow<'a, String>,
}
