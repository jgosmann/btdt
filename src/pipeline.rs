use crate::cache::Cache;
use crate::util::close::Close;
use std::io;
use std::io::{BufReader, BufWriter};
use std::path::Path;

#[derive(Debug)]
pub struct Pipeline<C: Cache> {
    cache: C,
}

impl<C: Cache> Pipeline<C> {
    pub fn new(cache: C) -> Self {
        Pipeline { cache }
    }

    pub fn restore(&self, keys: &[&str], destination: impl AsRef<Path>) -> io::Result<bool> {
        if let Some(reader) = self.cache.get(keys)? {
            tar::Archive::new(BufReader::new(reader)).unpack(destination.as_ref())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn store(&mut self, keys: &[&str], source: impl AsRef<Path>) -> io::Result<()> {
        let mut writer = BufWriter::new(self.cache.set(keys)?);
        {
            let mut archive = tar::Builder::new(&mut writer);
            archive.follow_symlinks(false);
            archive.append_dir_all(".", source)?;
            archive.finish()?;
        }
        writer.into_inner()?.close()?;
        Ok(())
    }

    pub fn into_cache(self) -> C {
        self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::local::LocalCache;
    use crate::pipeline::tests::fs_spec::{DirSpec, FileSpec, Node, SymlinkSpec};
    use crate::storage::in_memory::InMemoryStorage;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_roundtrip() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec {
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
        };

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();
        pipeline.store(&["cache-key"], &source_path).unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(spec.compare_with(&destination_path).unwrap(), vec![]);
    }

    mod fs_spec {
        use std::collections::HashMap;
        use std::fmt::{format, Debug};
        use std::fs::{DirBuilder, File, OpenOptions, Permissions};
        use std::io::{Read, Write};
        use std::os::unix;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        use std::path::{Path, PathBuf};
        use std::{fs, io};

        #[derive(Debug)]
        pub struct DirSpec {
            pub permissions: Permissions,
            pub children: HashMap<String, Box<dyn Node>>,
        }

        #[derive(Debug, Clone)]
        pub struct FileSpec {
            pub permissions: Permissions,
            pub content: Vec<u8>,
        }

        #[derive(Debug, Clone)]
        pub struct SymlinkSpec {
            pub target: PathBuf,
        }

        #[derive(Debug, PartialEq, Eq)]
        pub struct ComparisonMismatch {
            pub path: PathBuf,
            pub reason: String,
        }

        impl ComparisonMismatch {
            pub fn new(path: impl AsRef<Path>, reason: impl Into<String>) -> Self {
                ComparisonMismatch {
                    path: path.as_ref().to_owned(),
                    reason: reason.into(),
                }
            }
        }

        pub trait Node: Debug {
            fn create(&self, path: &Path) -> io::Result<()>;
            fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>>;
        }

        impl Node for DirSpec {
            fn create(&self, path: &Path) -> io::Result<()> {
                DirBuilder::new()
                    .mode(self.permissions.mode())
                    .create(path)?;
                for (name, child) in &self.children {
                    child.create(&path.join(name));
                }
                Ok(())
            }

            fn compare_with(&self, path: &Path) -> io::Result<Vec<ComparisonMismatch>> {
                if !path.is_dir() {
                    return Ok(vec![ComparisonMismatch::new(path, "not a directory")]);
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

                let mut mismatches = Vec::new();
                for (name, child) in &self.children {
                    mismatches.extend(child.compare_with(&path.join(name))?);
                }
                for dir in fs::read_dir(path)? {
                    let dir = dir?;
                    match dir.file_name().to_str() {
                        None => {
                            mismatches
                                .push(ComparisonMismatch::new(dir.path(), "non-UTF-8 file name"));
                        }
                        Some(file_name) => {
                            if !self.children.contains_key(file_name) {
                                mismatches.push(ComparisonMismatch::new(
                                    dir.path(),
                                    format!("additional file: '{}'", file_name),
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
    }
}
