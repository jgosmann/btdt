use btdt::cache::local::LocalCache;
use btdt::pipeline::Pipeline;
use btdt::storage::filesystem::FilesystemStorage;
use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use rand::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

struct IoBenchHarness<Rng: RngCore> {
    tempdir: tempfile::TempDir,
    pipeline: Pipeline<LocalCache<FilesystemStorage>>,
    input_path: PathBuf,
    rng: Rng,
}

impl Default for IoBenchHarness<Xoshiro256PlusPlus> {
    fn default() -> Self {
        let tempdir = tempdir().unwrap();
        let cache_path = tempdir.path().join("cache");
        std::fs::create_dir(&cache_path).unwrap();
        let cache = LocalCache::new(FilesystemStorage::new(cache_path));

        let input_path = tempdir.path().join("input");
        std::fs::create_dir(&input_path).unwrap();

        Self {
            tempdir,
            pipeline: Pipeline::new(cache),
            input_path,
            rng: Xoshiro256PlusPlus::seed_from_u64(42),
        }
    }
}

impl<Rng: RngCore> IoBenchHarness<Rng> {
    fn create_files(&mut self, num_files: usize, file_size: usize) {
        let input_path = self.tempdir.path().join("input");
        for i in 0..num_files {
            let mut file = File::create(&input_path.join(format!("file.{i}.bin"))).unwrap();
            const MAX_BUF_SIZE: usize = 10_485_760; // 10 MiB
            let mut buf = vec![0; usize::min(file_size, MAX_BUF_SIZE)];
            let mut remaining = file_size;
            while remaining > 0 {
                let slice = &mut buf[..usize::min(remaining, MAX_BUF_SIZE)];
                self.rng.fill_bytes(slice);
                file.write_all(slice).unwrap();
                remaining -= slice.len();
            }
        }
    }
}

pub fn store_small_files_benchmark(c: &mut Criterion) {
    let mut harness = IoBenchHarness::default();
    const FILE_SIZE: usize = 1024;

    let mut group = c.benchmark_group("I/O store many small files");
    group.sampling_mode(SamplingMode::Flat).sample_size(20);
    for num_files in [10, 100, 1000, 10_000] {
        harness.create_files(num_files, FILE_SIZE);
        group.throughput(Throughput::Bytes((num_files * FILE_SIZE) as u64));
        group.bench_function(format!("{num_files} files"), |b| {
            b.iter(|| {
                harness
                    .pipeline
                    .store(&["cache-key"], &harness.input_path)
                    .unwrap()
            })
        });
    }
    group.finish();
}

pub fn store_large_file_benchmark(c: &mut Criterion) {
    let mut harness = IoBenchHarness::default();

    let mut group = c.benchmark_group("I/O store large file");
    group.sampling_mode(SamplingMode::Flat).sample_size(20);
    #[allow(non_snake_case)]
    for file_size_MiB in [10u64, 100, 250, 500] {
        let file_size_bytes = file_size_MiB * 1024 * 1024;
        harness.create_files(1, file_size_bytes as usize);
        group.throughput(Throughput::Bytes(file_size_bytes));
        group.bench_function(format!("{file_size_MiB} MiB file"), |b| {
            b.iter(|| {
                harness
                    .pipeline
                    .store(&["cache-key"], &harness.input_path)
                    .unwrap()
            })
        });
    }
    group.finish();
}

pub fn restore_small_files_benchmark(c: &mut Criterion) {
    let mut harness = IoBenchHarness::default();
    const FILE_SIZE: usize = 1024;

    let mut group = c.benchmark_group("I/O restore many small files");
    group.sampling_mode(SamplingMode::Flat).sample_size(20);
    for num_files in [10, 100, 1000, 10_000] {
        harness.create_files(num_files, FILE_SIZE);
        harness
            .pipeline
            .store(&["cache-key"], &harness.input_path)
            .unwrap();

        group.throughput(Throughput::Bytes((num_files * FILE_SIZE) as u64));
        group.bench_function(format!("{num_files} files"), |b| {
            b.iter(|| {
                harness
                    .pipeline
                    .restore(&["cache-key"], tempdir().unwrap().path())
                    .unwrap()
            })
        });
    }
    group.finish();
}

pub fn restore_large_file_benchmark(c: &mut Criterion) {
    let mut harness = IoBenchHarness::default();

    let mut group = c.benchmark_group("I/O restore large file");
    group.sampling_mode(SamplingMode::Flat).sample_size(20);
    #[allow(non_snake_case)]
    for file_size_MiB in [10u64, 100, 250, 500] {
        let file_size_bytes = file_size_MiB * 1024 * 1024;
        harness.create_files(1, file_size_bytes as usize);
        harness
            .pipeline
            .store(&["cache-key"], &harness.input_path)
            .unwrap();

        group.throughput(Throughput::Bytes(file_size_bytes));
        group.bench_function(format!("{file_size_MiB} MiB file"), |b| {
            b.iter(|| {
                harness
                    .pipeline
                    .restore(&["cache-key"], tempdir().unwrap().path())
                    .unwrap()
            })
        });
    }
    group.finish();
}

criterion_group!(
    default_bench_config,
    restore_small_files_benchmark,
    restore_large_file_benchmark,
    store_small_files_benchmark,
    store_large_file_benchmark,
);
criterion_main!(default_bench_config);
