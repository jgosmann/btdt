//! Types to conveniently specify the contents of a file system tree for testing.
//!
//! # Examples
//!
//! ```rust
//! # use std::fs;
//! use std::fs::Permissions;
//! use std::os::unix::fs::PermissionsExt;
//! use std::path::PathBuf;
//! use btdt::test_util::fs_spec::{DirSpec, FileSpec, Node, SymlinkSpec};
//!
//! let tree = DirSpec {
//!     permissions: Permissions::from_mode(0o755),
//!     children: [
//!         (
//!             "file.txt".to_string(),
//!             Box::new(FileSpec {
//!                 permissions: Permissions::from_mode(0o644),
//!                 content: b"Hello, world!".to_vec(),
//!             }) as Box<dyn Node>,
//!         ),
//!         (
//!             "dir".to_string(),
//!             Box::new(DirSpec {
//!                 permissions: Permissions::from_mode(0o750),
//!                 children: [(
//!                     "exec-file".to_string(),
//!                     Box::new(FileSpec {
//!                         permissions: Permissions::from_mode(0o755),
//!                         content: b"#!/bin/sh\necho 'Hello, world!'\n".to_vec(),
//!                     }) as Box<dyn Node>,
//!                 )]
//!                 .into_iter()
//!                 .collect(),
//!             }),
//!         ),
//!         (
//!             "symlink".to_string(),
//!             Box::new(SymlinkSpec {
//!                 target: PathBuf::from("dir/exec-file"),
//!             }),
//!         ),
//!     ]
//!     .into_iter()
//!     .collect(),
//! };
//!
//! # const SPEC_PATH: &str = "/tmp/btdt-fs-spec";
//! # struct SpecDir;
//! # impl Drop for SpecDir {
//! #    fn drop(&mut self) {
//! #        fs::remove_dir_all(SPEC_PATH).expect(format!("Failed to remove directory at {}", SPEC_PATH).as_str());
//! #    }
//! # }
//! # let _spec_dir = SpecDir;
//! # let path = PathBuf::from(SPEC_PATH);
//! tree.create(&path).unwrap();
//! assert!(tree.compare_with(&path).unwrap().is_empty());

use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::{Read, Write};
use std::os::unix;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Specify a directory.
#[derive(Debug)]
pub struct DirSpec {
    /// The permissions of the directory.
    pub permissions: Permissions,
    /// The children of the directory.
    pub children: HashMap<String, Box<dyn Node>>,
}

/// Specify a file.
#[derive(Debug, Clone)]
pub struct FileSpec {
    /// The permissions of the file.
    pub permissions: Permissions,
    /// The content of the file.
    pub content: Vec<u8>,
}

/// Specify a symbolic link.
#[derive(Debug, Clone)]
pub struct SymlinkSpec {
    /// The target of the symbolic link.
    pub target: PathBuf,
}

/// Describes a mismatch between the specified file system tree and the actual filesystem.
#[derive(Debug, PartialEq, Eq)]
pub struct ComparisonMismatch {
    /// The path of the mismatch.
    pub path: PathBuf,
    /// The reason for the mismatch.
    pub reason: String,
}

impl ComparisonMismatch {
    fn new(path: impl AsRef<Path>, reason: impl Into<String>) -> Self {
        ComparisonMismatch {
            path: path.as_ref().to_owned(),
            reason: reason.into(),
        }
    }
}

/// A node in a file system tree.
pub trait Node: Debug {
    /// Creates the node at the given path in the actual file system.
    fn create(&self, path: &Path) -> io::Result<()>;

    /// Compares the node at the given path in the actual file system with this node.
    fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>>;
}

impl Node for DirSpec {
    fn create(&self, path: &Path) -> io::Result<()> {
        DirBuilder::new()
            .mode(self.permissions.mode())
            .create(path)?;
        for (name, child) in &self.children {
            child.create(&path.join(name))?;
        }
        Ok(())
    }

    fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>> {
        if !path.is_dir() {
            return Ok(vec![ComparisonMismatch::new(path, "not a directory")]);
        }
        let actual_permissions =
            Permissions::from_mode(fs::symlink_metadata(path)?.permissions().mode() & 0o7777);
        if actual_permissions != self.permissions {
            return Ok(vec![ComparisonMismatch::new(
                path,
                format!(
                    "permissions mismatch (expected: {:o}, actual: {:o})",
                    self.permissions.mode(),
                    actual_permissions.mode()
                ),
            )]);
        }

        let mut mismatches = Vec::new();
        for (name, child) in &self.children {
            mismatches.extend(child.compare_with(&path.join(name))?);
        }
        for dir in fs::read_dir(path)? {
            let dir = dir?;
            match dir.file_name().to_str() {
                None => {
                    mismatches.push(ComparisonMismatch::new(dir.path(), "non-UTF-8 file name"));
                }
                Some(file_name) => {
                    if !self.children.contains_key(file_name) {
                        mismatches.push(ComparisonMismatch::new(
                            dir.path(),
                            format!("additional file: '{file_name}'"),
                        ));
                    }
                }
            }
        }
        Ok(mismatches)
    }
}

impl Node for SymlinkSpec {
    fn create(&self, path: &Path) -> io::Result<()> {
        unix::fs::symlink(&self.target, path)?;
        Ok(())
    }

    fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>> {
        if !path.is_symlink() {
            return Ok(vec![ComparisonMismatch {
                path: path.to_path_buf(),
                reason: "not a symlink".into(),
            }]);
        }
        if path.read_link()? != self.target {
            return Ok(vec![ComparisonMismatch::new(path, "link target mismatch")]);
        }
        Ok(vec![])
    }
}

impl Node for FileSpec {
    fn create(&self, path: &Path) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .mode(self.permissions.mode())
            .open(path)?;
        file.write_all(&self.content)?;
        Ok(())
    }

    fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>> {
        match File::open(path) {
            Ok(mut file) => {
                if !path.is_file() {
                    return Ok(vec![ComparisonMismatch::new(path, "not a file")]);
                }
                let actual_permissions = Permissions::from_mode(
                    fs::symlink_metadata(path)?.permissions().mode() & 0o7777,
                );
                if actual_permissions != self.permissions {
                    return Ok(vec![ComparisonMismatch::new(
                        path,
                        format!(
                            "permissions mismatch (expected: {:o}, actual: {:o})",
                            self.permissions.mode(),
                            actual_permissions.mode()
                        ),
                    )]);
                }
                let mut actual_content = Vec::new();
                file.read_to_end(&mut actual_content)?;
                if actual_content == self.content {
                    Ok(vec![])
                } else {
                    Ok(vec![ComparisonMismatch::new(path, "content mismatch")])
                }
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    Ok(vec![ComparisonMismatch::new(path, "file not found")])
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl DirSpec {
    /// Creates a filesystem tree specification to serve as test fixture on Unix systems.
    ///
    /// This fixture covers files, directories, symbolic links, and different permissions,
    /// in detail:
    ///
    /// - A file named `file.txt` with content `Hello, world!` and permissions `644`.
    /// - A directory named `dir` with permissions `750` containing:
    ///   - A file named `exec-file` with content `#!/bin/sh\necho 'Hello, world!'\n` and permissions
    ///     `755`.
    /// - A symbolic link named `symlink` pointing to `dir/exec-file`.
    pub fn create_unix_fixture() -> Self {
        Self {
            permissions: Permissions::from_mode(0o755),
            children: [
                (
                    "file.txt".to_string(),
                    Box::new(FileSpec {
                        permissions: Permissions::from_mode(0o644),
                        content: b"Hello, world!".to_vec(),
                    }) as Box<dyn Node>,
                ),
                (
                    "dir".to_string(),
                    Box::new(DirSpec {
                        permissions: Permissions::from_mode(0o750),
                        children: [(
                            "exec-file".to_string(),
                            Box::new(FileSpec {
                                permissions: Permissions::from_mode(0o755),
                                content: b"#!/bin/sh\necho 'Hello, world!'\n".to_vec(),
                            }) as Box<dyn Node>,
                        )]
                        .into_iter()
                        .collect(),
                    }),
                ),
                (
                    "symlink".to_string(),
                    Box::new(SymlinkSpec {
                        target: PathBuf::from("dir/exec-file"),
                    }),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
}
