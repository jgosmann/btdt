use nanoid::nanoid;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};

pub struct StagedFile<P: AsRef<Path>> {
    file: File,
    tmp_path: PathBuf,
    target_path: P,
}

impl<P: AsRef<Path>> StagedFile<P> {
    pub fn new(target_path: P) -> io::Result<Self> {
        Self::new_with_suffix(target_path, &nanoid!(6))
    }

    fn new_with_suffix(target_path: P, suffix: &str) -> io::Result<Self> {
        let filename = target_path
            .as_ref()
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or(io::Error::new(ErrorKind::InvalidInput, "Invalid filename"))?;
        let tmp_path = target_path
            .as_ref()
            .with_file_name(format!("{}.{}", filename, suffix));
        let file = File::create_new(&tmp_path)?;
        Ok(Self {
            file,
            tmp_path,
            target_path,
        })
    }
}

impl<P: AsRef<Path>> Write for StagedFile<P> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl<P: AsRef<Path>> Drop for StagedFile<P> {
    fn drop(&mut self) {
        fs::rename(&self.tmp_path, self.target_path.as_ref())
            .expect("Failed to rename temporary file to target path");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn test_file_not_available_until_drop() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("test.txt");
        let mut file = StagedFile::new(&path).unwrap();
        file.write_all("Hello, world!".as_bytes()).unwrap();
        assert!(!path.exists());

        drop(file);
        let mut buf = String::new();
        File::open(path).unwrap().read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "Hello, world!");
    }

    #[test]
    fn test_fails_if_staging_file_exists() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("test.txt");
        let suffix = "suffix";
        let mut file = StagedFile::new_with_suffix(&path, suffix).unwrap();
        file.write_all("Hello, world!".as_bytes()).unwrap();
        assert!(StagedFile::new_with_suffix(&path, suffix).is_err());
    }
}
