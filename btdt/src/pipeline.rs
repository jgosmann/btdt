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
    use crate::storage::in_memory::InMemoryStorage;
    use crate::test_util::fs_spec::{DirSpec, Node};
    use tempfile::tempdir;

    #[test]
    fn test_roundtrip() {
        let cache = LocalCache::new(InMemoryStorage::new());
        let mut pipeline = Pipeline::new(cache);

        let spec = DirSpec::create_unix_fixture();

        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source-root");
        spec.create(source_path.as_ref()).unwrap();
        pipeline.store(&["cache-key"], &source_path).unwrap();

        let destination_path = tempdir.path().join("destination-root");
        pipeline.restore(&["cache-key"], &destination_path).unwrap();

        assert_eq!(spec.compare_with(&destination_path).unwrap(), vec![]);
    }
}
