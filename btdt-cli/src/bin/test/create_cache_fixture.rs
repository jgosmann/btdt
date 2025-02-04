use anyhow::{anyhow, Context};
use btdt::cache::blob_id::BlobIdFactory;
use btdt::cache::local::LocalCache;
use btdt::pipeline::Pipeline;
use btdt::storage::filesystem::FilesystemStorage;
use clap::Parser;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::fs::{create_dir, read_dir, remove_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};
use tempfile::tempdir;

/// Creates the cache fixture for the CLI integration tests.
#[derive(Parser)]
struct CliOpts {
    /// Directory containing the CLI tests.
    output_dir: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let cli_opts = CliOpts::parse();

    if !cli_opts.output_dir.join("_cache-fixture").is_dir() {
        return Err(anyhow!("No '_cache-fixture' directory found in '{}'. Did you provide the correct path? No changes will be made.", cli_opts.output_dir.display()));
    }

    let cache_dir = cli_opts.output_dir.join("_cache-fixture");
    remove_dir_all(&cache_dir)?;
    create_dir(&cache_dir)?;
    let mut cache_pipeline = Pipeline::new(LocalCache::with_blob_id_factory(
        FilesystemStorage::new(cache_dir.clone()),
        BlobIdFactory::new(StdRng::from_seed([0; 32])),
    ));

    let tmp = tempdir()?;
    {
        let mut file = File::create_new(tmp.path().join("a.txt"))?;
        file.write_all(b"lorem ipsum\n")?;
    }
    cache_pipeline.store(&["cache-key-0", "cache-key-1"], &tmp.path())?;

    let tmp = tempdir()?;
    {
        let mut file = File::create_new(tmp.path().join("b.txt"))?;
        file.write_all(b"wrong file restored\n")?;
    }
    cache_pipeline.store(&["other-cache-key"], &tmp.path())?;

    for test_dir in [
        "restore-first-matched-key.in",
        "restore-first-matched-key-comma-separated.in",
        "restore-non-existent-key.in",
    ] {
        let path = cli_opts.output_dir.join(test_dir).join("cache");
        match remove_dir_all(&path) {
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(err)
                }
            }
            Ok(_) => Ok(()),
        }
        .with_context(|| format!("path: {}", path.display()))?;
        copy_dir(&cache_dir, &path).with_context(|| format!("path: {}", path.display()))?;
    }

    Ok(())
}

fn copy_dir<P: AsRef<Path>>(src: P, dst: P) -> io::Result<()> {
    create_dir(dst.as_ref())?;
    for entry in read_dir(src.as_ref())? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            unimplemented!("Unsupported file type: {:?}", entry.file_type());
        }
    }
    Ok(())
}
