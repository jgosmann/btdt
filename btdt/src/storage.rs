//! A storage is a place where files are stored, for example the local filesystem.
//!
//! This module defines the `Storage` trait and provides implementations of it in its submodules.

pub mod filesystem;
pub mod in_memory;
#[cfg(test)]
pub mod tests;

use super::util::close::Close;
use std::borrow::Cow;
use std::io;
use std::io::{Read, Write};

/// A storage is a place where files are stored, for example the local filesystem.
///
/// Paths are expected to use `/` as the path separator and to be absolute within the storage
/// (this could, however, map to a path relative to some base directory within the filesystem).
///
/// For reading and writing files, a [Storage::Reader] and [Storage::Writer] are returned,
/// respectively.
/// This allows interacting with the file data without loading the full file content into memory.
/// However, as a written file must become available atomically, the `Writer` must implement
/// [Close] to finalize the write operation.
///
/// Note that the storage implementation must support parallel access from other `btdt` processes.
/// This means that even the return values of the trait methods may change in-between calls.
/// For implementors, it means that a sufficient degree of atomicity must be ensured, especially
/// when reading or writing files.
pub trait Storage {
    /// The type of reader returned by this storage.
    type Reader: Read;

    /// The type of writer returned by this storage.
    type Writer: Write + Close;

    /// Deletes the file at the given path.
    fn delete(&self, path: &str) -> io::Result<()>;

    /// Checks if a file exists at the given path.
    fn exists_file(&self, path: &str) -> io::Result<bool>;

    /// Returns a reader for the file at the given path.
    fn get(&self, path: &str) -> io::Result<Self::Reader>;

    /// Returns an iterator over the entries in the directory at the given path.
    fn list(&self, path: &str) -> io::Result<impl Iterator<Item = io::Result<StorageEntry>>>;

    /// Returns a writer for the file at the given path.
    ///
    /// The file is created if it does not exist, and truncated if it does.
    /// The writer must be finalized by calling [Close::close] to make the file available.
    ///
    /// The implementation must ensure that the file becomes available atomically when
    /// [Close::close] is called. It also must create intermediate directories if necessary.
    fn put(&self, path: &str) -> io::Result<Self::Writer>;
}

/// The type of entry when listing a storage directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    /// A standard file.
    File,
    /// A directory.
    Directory,
}

/// An entry in a storage directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageEntry<'a> {
    /// The type of the entry.
    pub entry_type: EntryType,
    /// The (file) name of the entry.
    pub name: Cow<'a, String>,
    /// The file size of the entry in bytes.
    ///
    /// This is `0` for directories.
    pub size: u64,
}
