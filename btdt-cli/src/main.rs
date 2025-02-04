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
    /// Calculate the hash of a file.
    Hash {
        /// File to hash.
        path: PathBuf,
    },

    /// Restore cached files.
    ///
    /// The first key that exists in the cache will be used.
    Restore {
        #[command(flatten)]
        cache_ref: CacheRef,

        /// Directory to restore the files to.
        destination_dir: PathBuf,
    },

    /// Store files in the cache.
    ///
    /// The cached files will be accessible under all specified keys.
    /// Existing keys will be overwritten.
    Store {
        #[command(flatten)]
        cache_ref: CacheRef,

        /// Directory to store in the cache.
        source_dir: PathBuf,
    },
}

/// Reference to cache entries defining the cache to use and the keys in the cache to operate on.
#[derive(Args)]
struct CacheRef {
    /// Keys to operate on.
    #[arg(short, long, required = true, value_delimiter = ',')]
    keys: Vec<String>,

    /// Path to the cache directory.
    #[arg(short, long)]
    cache: String,
}

impl CacheRef {
    fn keys(&self) -> Vec<&str> {
        self.keys
            .iter()
            .filter(|k| !k.is_empty())
            .map(String::as_str)
            .collect()
    }

    fn to_pipeline(&self) -> Result<Pipeline<LocalCache<FilesystemStorage>>, anyhow::Error> {
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
        Ok(Pipeline::new(LocalCache::new(storage)))
    }
}

fn main() -> Result<ExitCode, anyhow::Error> {
    let cli_opts = CliOpts::parse();
    match cli_opts.command {
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
            cache_ref,
            source_dir,
        } => {
            cache_ref
                .to_pipeline()?
                .store(&cache_ref.keys(), &source_dir)
                .with_context(|| format!("Could not cache: {}", source_dir.display()))?;
        }
        Commands::Restore {
            cache_ref,
            destination_dir,
        } => {
            if !cache_ref
                .to_pipeline()?
                .restore(&cache_ref.keys(), &destination_dir)
                .with_context(|| format!("Could not restore to: {}", destination_dir.display()))?
            {
                eprintln!("Keys not found in cache.");
                return Ok(ExitCode::from(2));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
