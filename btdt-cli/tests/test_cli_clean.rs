use crate::cache_fixture::CacheFixture;
use btdt::cache::Cache;
use btdt::cache::local::LocalCache;
use btdt::storage::filesystem::FilesystemStorage;
use std::fs::{File, create_dir_all};
use std::process::Command;

mod cache_fixture;

#[test]
fn test_clean_removes_entries_based_on_max_age() {
    let cache_fixture = CacheFixture::new().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
        .arg("clean")
        .arg("--cache")
        .arg(cache_fixture.path().to_str().unwrap())
        .arg("--max-age")
        .arg("0d")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cache = LocalCache::new(FilesystemStorage::new(cache_fixture.path().to_path_buf()));
    assert!(
        cache
            .get(&["cache-key-0", "cache-key-1", "other-cache-key"])
            .unwrap()
            .is_none(),
        "expected all entries to be removed in cache"
    );
}

#[test]
fn test_clean_removes_entries_based_on_max_size() {
    let cache_fixture = CacheFixture::new().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
        .arg("clean")
        .arg("--cache")
        .arg(cache_fixture.path().to_str().unwrap())
        .arg("--max-size")
        .arg("0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cache = LocalCache::new(FilesystemStorage::new(cache_fixture.path().to_path_buf()));
    assert!(
        cache
            .get(&["cache-key-0", "cache-key-1", "other-cache-key"])
            .unwrap()
            .is_none(),
        "expected all entries to be removed in cache"
    );
}

#[test]
fn test_clean_removes_stale_temporary_files() {
    let cache_fixture = CacheFixture::new().unwrap();
    create_dir_all(cache_fixture.path().join("blob/aq")).unwrap();
    let tmp_file_path = cache_fixture
        .path()
        .join("blob/aq/u2ho0j2b0bi5qargss3lqvl2.tmp.1234567");
    File::create_new(&tmp_file_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
        .arg("clean")
        .arg("--cache")
        .arg(cache_fixture.path().to_str().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "clean failed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !tmp_file_path.exists(),
        "expected temporary file to be removed from cache"
    );
}
