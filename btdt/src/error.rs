//! Error types used by btdt.

use std::io;
use std::path::PathBuf;

pub type IoPathResult<T> = Result<T, IoPathError>;

/// An I/O error that optionally includes the path that the error occurred on.
#[derive(Debug)]
pub struct IoPathError {
    error: io::Error,
    path: Option<PathBuf>,
}

impl std::fmt::Display for IoPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(path) => write!(f, "I/O error on path {}: {}", path.display(), self.error),
            None => write!(f, "{}", self.error),
        }
    }
}

impl std::error::Error for IoPathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<IoPathError> for io::Error {
    fn from(value: IoPathError) -> Self {
        value.error
    }
}

impl IoPathError {
    /// Creates a new `IoPathError` with the given error and path.
    pub fn new(error: io::Error, path: impl Into<PathBuf>) -> Self {
        Self {
            error,
            path: Some(path.into()),
        }
    }

    /// Creates a new `IoPathError` with the given error and no path.
    pub fn new_no_path(error: io::Error) -> Self {
        Self { error, path: None }
    }

    /// Returns a reference to the underlying `io::Error`.
    pub fn io_error(&self) -> &io::Error {
        &self.error
    }

    /// Consumes the `IoPathError` and returns the underlying `io::Error`.
    pub fn into_io_error(self) -> io::Error {
        self.error
    }

    /// Returns a reference to the path associated with the error.
    pub fn path(&self) -> &Option<PathBuf> {
        &self.path
    }
}

/// Extension trait to convert `io::Result<T>` into `IoPathResult<T>`, optionally adding a path.
pub trait WithPath<T> {
    /// Converts the `io::Result<T>` into an `IoPathResult<T>` without a path.
    fn no_path(self) -> IoPathResult<T>;
    /// Converts the `io::Result<T>` into an `IoPathResult<T>`, adding the given path.
    fn with_path(self, path: impl Into<PathBuf>) -> IoPathResult<T>;
}

impl<T> WithPath<T> for io::Result<T> {
    fn no_path(self) -> IoPathResult<T> {
        self.map_err(IoPathError::new_no_path)
    }

    fn with_path(self, path: impl Into<PathBuf>) -> IoPathResult<T> {
        self.map_err(|e| IoPathError::new(e, path))
    }
}
