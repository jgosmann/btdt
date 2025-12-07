//! Utility functions for file system operations in tests.

use rand::RngCore;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;

/// Extension trait for creating files filled with random data.
pub trait CreateFilled {
    /// Creates a new file at the given path, filled with random data of the given size.
    fn create_filled(path: &Path, size: usize, rng: &mut impl RngCore) -> io::Result<File>;
}

impl CreateFilled for File {
    fn create_filled(path: &Path, size: usize, rng: &mut impl RngCore) -> io::Result<File> {
        let mut file = File::create(path)?;
        const MAX_BUF_SIZE: usize = 10_485_760; // 10 MiB
        let mut buf = vec![0; usize::min(size, MAX_BUF_SIZE)];
        let mut remaining = size;
        while remaining > 0 {
            let slice = &mut buf[..usize::min(remaining, MAX_BUF_SIZE)];
            rng.fill_bytes(slice);
            file.write_all(slice)?;
            remaining -= slice.len();
        }
        file.flush()?;
        Ok(file)
    }
}
