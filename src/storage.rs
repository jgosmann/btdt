pub mod filesystem;
pub mod in_memory;
#[cfg(test)]
pub mod tests;

use std::borrow::Cow;
use std::io;
use std::io::{Read, Write};

pub trait Storage {
    fn delete(&mut self, path: &str) -> io::Result<()>;
    fn get(&self, path: &str) -> io::Result<impl Read>;
    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>>;
    fn put(&mut self, path: &str) -> io::Result<impl Write>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageEntry<'a> {
    entry_type: EntryType,
    name: Cow<'a, String>,
}
