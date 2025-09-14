use std::io;
use std::path::PathBuf;

pub type IoPathResult<T> = Result<T, IoPathError>;

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
    pub fn new(error: io::Error, path: impl Into<PathBuf>) -> Self {
        Self {
            error,
            path: Some(path.into()),
        }
    }

    pub fn new_no_path(error: io::Error) -> Self {
        Self { error, path: None }
    }

    pub fn io_error(&self) -> &io::Error {
        &self.error
    }

    pub fn into_io_error(self) -> io::Error {
        self.error
    }

    pub fn path(&self) -> &Option<PathBuf> {
        &self.path
    }
}

pub trait WithPath<T> {
    fn no_path(self) -> IoPathResult<T>;
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
