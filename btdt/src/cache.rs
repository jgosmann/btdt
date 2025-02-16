//! A cache manages keys and associated data, and might use a storage to store that data.
//!
//! This module defines the `Cache` trait and provides implementations of it in its submodules.

use crate::util::close::Close;
use std::io;
use std::io::{Read, Write};

pub mod blob_id;
pub mod local;
mod meta;

/// A cache manages keys and associated data.
///
/// Data can be stored under one or more keys, and retrieved by any of those keys.
/// If a key is reused, the data will be overwritten for that key.
///
/// For reading and writing data, [Cache::Reader] and [Cache::Writer] are returned, respectively.
/// This allows interacting with the data without loading the full content into memory.
/// However, as a written data must become available atomically, the `Writer` must implement
/// [Close] to finalize the write operation.
pub trait Cache {
    /// The type of reader returned by this cache.
    type Reader: Read;

    /// The type of writer returned by this cache.
    type Writer: Write + Close;

    /// Returns a reader for the data stored under the first given key found in the cache. If none
    /// of the keys is found, `Ok(None)` is returned.
    fn get(&self, keys: &[&str]) -> io::Result<Option<Self::Reader>>;

    /// Returns a writer for the data to be stored under all the given keys.
    ///
    /// If a key already exists, its data will be overwritten.
    ///
    /// The writer must be finalized by calling [Close::close] to make the data available
    /// atomically.
    fn set(&mut self, keys: &[&str]) -> io::Result<Self::Writer>;
}
