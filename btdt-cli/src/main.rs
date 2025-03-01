mod humanbytes;

use anyhow::Context;
use btdt::cache::local::LocalCache;
use btdt::pipeline::Pipeline;
use btdt::storage::filesystem::FilesystemStorage;
use clap::{Args, Parser, Subcommand};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

/// "been there, done that" - a tool for flexible CI caching
///
/// This tool is designed to be used in CI pipelines to cache build artifacts and dependencies.
/// It is agnostic to the CI environment and is a simple command-line tool that can be integrated
/// into any pipeline.
///
/// Cached files can be stored in the local filesystem (e.g. mounted from a persistent volume in
/// Kubernetes/Tekton, or a workspace folder in Jenkins).
#[derive(Parser)]
#[command(version)]
struct CliOpts {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Clean old entries from cache.
    Clean {
        #[command(flatten)]
        cache_ref: CacheRef,

        /// Maximum age of last access before entries are deleted.
        ///
        /// Supports human-readable units like "1d" for one day.
        #[arg(long)]
        max_age: Option<humantime::Duration>,

        /// Maximum size of the cache before entries are deleted.
        ///
        /// Supports human-readable units like "1GiB" for one gibibyte or "1GB" for one gigabyte.
        /// The "B" for bytes may be omitted.
        ///
        /// This doesn't account for metadata, thus the overall cache size may be a bit larger.
        #[arg(long, value_parser=humanbytes::parse_bytes_from_str)]
        max_size: Option<u64>,
    },

    /// Calculate the hash of a file.
    Hash {
        /// File to hash.
        path: PathBuf,
    },

    /// Restore cached files.
    ///
    /// The first key that exists in the cache will be used.
    ///
    /// # Exit codes:
    ///
    /// - 0: Files were restored from the primary (i.e. first listed) cache key.
    /// - 1: Error in command invocation.
    /// - 2: No keys were found in the cache.
    /// - 3: Files were restored from a non-primary cache key.
    Restore {
        #[command(flatten)]
        entries_ref: CacheEntriesRef,

        /// Directory to restore the files to.
        destination_dir: PathBuf,

        /// Exit with success status code if any key is found in the cache.
        ///
        /// Usually, the success exit code is only returned if the primary key (i.e. first listed
        /// key) is found in the cache, and 3 is returned if another key was restored.
        #[arg(long, action)]
        success_rc_on_any_key: bool,
    },

    /// Store files in the cache.
    ///
    /// The cached files will be accessible under all specified keys.
    /// Existing keys will be overwritten.
    Store {
        #[command(flatten)]
        entries_ref: CacheEntriesRef,

        /// Directory to store in the cache.
        source_dir: PathBuf,
    },
}

/// Reference to cache entries defining the cache to use and the keys in the cache to operate on.
#[derive(Args)]
struct CacheEntriesRef {
    /// Keys to operate on.
    #[arg(short, long, required = true, value_delimiter = ',')]
    keys: Vec<String>,

    #[command(flatten)]
    cache_ref: CacheRef,
}

/// Reference to the cache to use.
#[derive(Args)]
struct CacheRef {
    /// Path to the cache directory.
    #[arg(short, long)]
    cache: String,
}

impl CacheEntriesRef {
    fn keys(&self) -> Vec<&str> {
        self.keys
            .iter()
            .filter(|k| !k.is_empty())
            .map(String::as_str)
            .collect()
    }

    fn to_pipeline(&self) -> Result<Pipeline<LocalCache<FilesystemStorage>>, anyhow::Error> {
        Ok(Pipeline::new(self.cache_ref.to_cache()?))
    }
}

impl CacheRef {
    fn to_cache(&self) -> Result<LocalCache<FilesystemStorage>, anyhow::Error> {
        let path = PathBuf::from(&self.cache)
            .canonicalize()
            .and_then(|path| {
                if !path.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "Not a directory",
                    ));
                }
                Ok(path)
            })
            .with_context(|| format!("Could not access cache: {}", &self.cache))?;
        let storage = FilesystemStorage::new(path);
        Ok(LocalCache::new(storage))
    }
}

fn main() -> Result<ExitCode, anyhow::Error> {
    let cli_opts = CliOpts::parse();
    match cli_opts.command {
        Commands::Clean {
            cache_ref,
            max_age,
            max_size,
        } => {
            let mut cache = cache_ref.to_cache()?;
            cache.clean(
                max_age
                    .map(|max_age| chrono::TimeDelta::from_std(*max_age.as_ref()))
                    .transpose()?,
                max_size,
            )?;
            cache.into_storage().clean_leftover_tmp_files()?;
        }
        Commands::Hash { path } => {
            let file =
                File::open(&path).with_context(|| format!("Could not open: {}", path.display()))?;
            println!(
                "{}",
                blake3::Hasher::new()
                    .update_reader(file)?
                    .finalize()
                    .to_hex()
            );
        }
        Commands::Store {
            entries_ref,
            source_dir,
        } => {
            entries_ref
                .to_pipeline()?
                .store(&entries_ref.keys(), &source_dir)
                .with_context(|| format!("Could not cache: {}", source_dir.display()))?;
        }
        Commands::Restore {
            entries_ref,
            destination_dir,
            success_rc_on_any_key,
        } => {
            if let Some(restored_key) = entries_ref
                .to_pipeline()?
                .restore(&entries_ref.keys(), &destination_dir)
                .with_context(|| format!("Could not restore to: {}", destination_dir.display()))?
            {
                println!("Restored key {}", restored_key);
                let primary_key = entries_ref.keys.first().map(String::as_str);
                if !success_rc_on_any_key && Some(restored_key) != primary_key {
                    return Ok(ExitCode::from(3));
                }
            } else {
                eprintln!("Keys not found in cache.");
                return Ok(ExitCode::from(2));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
