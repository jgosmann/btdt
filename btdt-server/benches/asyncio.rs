use btdt::test_util::fs::CreateFilled;
use btdt_server_lib::asyncio::StreamAdapter;
use criterion::{Criterion, criterion_group, criterion_main};
use rand_core::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::fs::File;
use std::hint::black_box;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use tokio_stream::StreamExt;

struct BenchHarness<Rng: RngCore> {
    tempdir: TempDir,
    rng: Rng,
    runtime: Runtime,
}

impl Default for BenchHarness<Xoshiro256PlusPlus> {
    fn default() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        Self {
            tempdir,
            rng: Xoshiro256PlusPlus::seed_from_u64(42),
            runtime: Runtime::new().unwrap(),
        }
    }
}

pub fn bench_stream_adapter(c: &mut Criterion) {
    let mut harness = BenchHarness::default();
    let mut group = c.benchmark_group("StreamAdapter");
    #[allow(non_snake_case)]
    for size_kB in [1, 10, 1 * 1024, 10 * 1024, 100 * 1024] {
        let size = size_kB * 1024;
        let input_path = harness.tempdir.path().join("input");
        File::create_filled(&input_path, size, &mut harness.rng).unwrap();

        group.throughput(criterion::Throughput::Bytes(size as u64));
        group.bench_function(format!("Read {} KiB bytes", size_kB), |b| {
            b.to_async(&harness.runtime).iter(async || {
                let mut stream_adapter = StreamAdapter::new(
                    Box::new(File::open(&input_path).unwrap()),
                    Some(size as u64),
                );
                while let Some(chunk) = stream_adapter.next().await {
                    black_box(chunk.unwrap());
                }
            })
        });
    }
    group.finish();
}

criterion_group!(default_bench_config, bench_stream_adapter,);
criterion_main!(default_bench_config);
