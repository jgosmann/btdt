use anyhow::Context;
use blake3::{Hash, Hasher};
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub fn hash_path(path: &Path) -> anyhow::Result<Hash> {
    if path.is_dir() {
        return hash_dir(path);
    }
    hash_file(path)
}

fn hash_dir(path: &Path) -> anyhow::Result<Hash> {
    let mut entries: Vec<_> = path
        .read_dir()
        .with_context(|| format!("Could not read directory: {}", path.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut hasher = Hasher::new();
    for entry in entries {
        hasher.update(entry.file_name().as_bytes());
        hasher.update(hash_path(&entry.path())?.as_bytes());
    }
    Ok(hasher.finalize())
}

fn hash_file(path: &Path) -> anyhow::Result<Hash> {
    let file = File::open(path).with_context(|| format!("Could not open: {}", path.display()))?;
    let mut hasher = Hasher::new();
    hasher.update_reader(file)?;
    Ok(hasher.finalize())
}
