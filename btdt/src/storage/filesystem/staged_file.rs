use crate::util::close::Close;
use crate::util::encoding::ICASE_NOPAD_ALPHANUMERIC_ENCODING;
use data_encoding::Encoding;
use fs2::FileExt;
use rand::{CryptoRng, RngCore};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};

const TMP_FILE_SUFFIX_ENCODING: Encoding = ICASE_NOPAD_ALPHANUMERIC_ENCODING;
const TMP_FILE_SUFFIX_BYTES: usize = 4;
const TMP_FILE_SUFFIX_ENCODED_LEN: usize = 7;

/// A file that is staged to be atomically moved to a target path.
///
/// The file is created with a temporary name in the same directory as the target path.
/// Once [Close::close] is called or the instance is dropped, the file is moved to the target path.
pub struct StagedFile<P: AsRef<Path>> {
    file: File,
    tmp_path: PathBuf,
    target_path: P,
    finalized: bool,
}

impl<P: AsRef<Path>> StagedFile<P> {
    pub fn new<R: CryptoRng + RngCore>(target_path: P, rng: &mut R) -> io::Result<Self> {
        let mut bytes = [0; TMP_FILE_SUFFIX_BYTES];
        rng.fill_bytes(&mut bytes);
        Self::new_with_suffix(
            target_path,
            &TMP_FILE_SUFFIX_ENCODING.encode(bytes.as_ref()),
        )
    }

    fn new_with_suffix(target_path: P, suffix: &str) -> io::Result<Self> {
        let filename = target_path
            .as_ref()
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or(io::Error::new(ErrorKind::InvalidInput, "Invalid filename"))?;
        let tmp_path = target_path
            .as_ref()
            .with_file_name(format!("{}.tmp.{}", filename, suffix));
        for _ in 0..5 {
            let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp_path)?;
            file.lock_exclusive()?;
            if !tmp_path.exists() {
                // The file was deleted, likely by clean_leftover_tmp_files, before we could lock
                // it. Try again.
                continue;
            }
            return Ok(Self {
                file,
                tmp_path,
                target_path,
                finalized: false,
            });
        }
        Err(io::Error::new(
            ErrorKind::Other,
            "Failed to create and lock temporary file",
        ))
    }

    fn finalize(&mut self) -> io::Result<()> {
        self.finalized = true;
        fs::rename(&self.tmp_path, self.target_path.as_ref())
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

impl<P: AsRef<Path>> Close for StagedFile<P> {
    fn close(mut self) -> io::Result<()> {
        self.finalize()
    }
}

impl<P: AsRef<Path>> Drop for StagedFile<P> {
    fn drop(&mut self) {
        if !self.finalized {
            self.finalize()
                .expect("Failed to move temporary file to target path");
        }
    }
}

/// Cleans up leftover temporary files of [StagedFile] in the given directory and its
/// subdirectories.
///
/// Usually the temporary file will be deleted when the [StagedFile] is closed or dropped. However,
/// if a process is killed hard, the temporary file may be left behind.
pub fn clean_leftover_tmp_files<P_: AsRef<Path>>(path: P_) -> io::Result<()> {
    for entry in path.as_ref().read_dir()? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            if let Some(file_name) = entry.file_name().to_str() {
                let mut parts = file_name.rsplitn(3, '.');
                let suffix = parts.next();
                let ext = parts.next();
                if ext == Some("tmp")
                    && suffix.map(|s| s.len()) == Some(TMP_FILE_SUFFIX_ENCODED_LEN)
                {
                    let is_locked = OpenOptions::new()
                        .read(true)
                        .open(entry.path())
                        .and_then(|file_handle| file_handle.try_lock_exclusive())
                        .is_ok();
                    if is_locked {
                        fs::remove_file(entry.path())?;
                    }
                }
            }
        } else if file_type.is_dir() {
            clean_leftover_tmp_files(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn test_tmp_file_suffix_encoded_is_accurate() {
        assert_eq!(
            TMP_FILE_SUFFIX_ENCODED_LEN,
            TMP_FILE_SUFFIX_ENCODING
                .encode(&[0; TMP_FILE_SUFFIX_BYTES])
                .len()
        );
    }

    #[test]
    fn test_file_not_available_until_drop() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("test.txt");
        let mut file = StagedFile::new(&path, &mut StdRng::seed_from_u64(0)).unwrap();
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

    #[test]
    fn test_can_close_and_drop() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("test.txt");
        {
            let mut file = StagedFile::new(&path, &mut StdRng::seed_from_u64(0)).unwrap();
            file.write_all("Hello, world!".as_bytes()).unwrap();
            file.close().unwrap();
        }
    }

    #[test]
    fn test_clean_leftover_tmp_files_removes_leftover_tmp_files() {
        let tempdir = tempdir().unwrap();
        let subdir_path = tempdir.path().join("subdir");
        fs::create_dir(&subdir_path).unwrap();
        let path = subdir_path.join(format!(
            "test.tmp.{}",
            TMP_FILE_SUFFIX_ENCODING.encode(&[0; TMP_FILE_SUFFIX_BYTES])
        ));
        File::create(&path).unwrap();
        clean_leftover_tmp_files(tempdir.path()).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_clean_leftover_tmp_files_does_not_remove_files_still_in_use() {
        let tempdir = tempdir().unwrap();
        let target_path = tempdir.path().join("test.txt");
        {
            let file = StagedFile::new(&target_path, &mut StdRng::seed_from_u64(0)).unwrap();
            clean_leftover_tmp_files(tempdir.path()).unwrap();
            assert!(tempdir.path().read_dir().unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .starts_with("test.txt.tmp.")
            }));
        }
        assert!(target_path.exists());
    }
}
