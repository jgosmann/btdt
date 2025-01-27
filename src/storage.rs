pub mod filesystem;
pub mod in_memory;
#[cfg(test)]
pub mod tests;

use super::close::Close;
use std::borrow::Cow;
use std::io;
use std::io::{Read, Write};

pub trait Captures<U> {}
impl<T: ?Sized, U> Captures<U> for T {}

pub trait Storage {
    type Reader<'a>: Read + Captures<&'a ()>
    where
        Self: 'a;
    type Writer<'a>: Write + Close + Captures<&'a ()>
    where
        Self: 'a;

    fn delete(&mut self, path: &str) -> io::Result<()>;
    fn exists_file(&mut self, path: &str) -> io::Result<bool>;
    fn get<'a>(&'a self, path: &str) -> io::Result<Self::Reader<'a>>;
    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>>;
    fn put<'a>(&'a mut self, path: &str) -> io::Result<Self::Writer<'a>>;
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
