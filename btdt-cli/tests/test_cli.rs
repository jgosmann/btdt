use crate::cache_fixture::CacheFixture;
use btdt::test_util::fs_spec::{DirSpec, Node};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

mod cache_fixture;

#[test]
fn test_cmd() {
    let cache_fixture = CacheFixture::new().unwrap();
    for test_dir in [
        "clean-supports-human-units.in",
        "restore-first-matched-key.in",
        "restore-first-matched-key-comma-separated.in",
        "restore-non-existent-key.in",
    ] {
        cache_fixture
            .copy_to(PathBuf::from("tests/cli").join(test_dir).join("cache"))
            .unwrap();
    }
    trycmd::TestCases::new()
        .case("tests/cli/*.md")
        .case("tests/cli/*.toml")
        .run();
}

#[test]
fn test_roundtrip() {
    let tempdir = tempdir().unwrap();
    let cache_path = tempdir.path().join("cache");
    let source_path = tempdir.path().join("source-root");
    let destination_paths = [
        tempdir.path().join("destination-root-0"),
        tempdir.path().join("destination-root-1"),
        tempdir.path().join("destination-root-2"),
    ];

    let spec = DirSpec::create_unix_fixture();
    spec.create(source_path.as_ref()).unwrap();
    fs::create_dir(&cache_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
        .arg("store")
        .arg("--cache")
        .arg(cache_path.to_str().unwrap())
        .arg("--keys")
        .arg("cache-key-0")
        .arg("--keys")
        .arg("cache-key-1,,cache-key-2")
        .arg(&source_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "store failed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for (i, destination_path) in destination_paths.iter().enumerate() {
        let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
            .arg("restore")
            .arg("--cache")
            .arg(cache_path.to_str().unwrap())
            .arg("--keys")
            .arg(format!("cache-key-{}", i))
            .arg(&destination_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "restore failed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(spec.compare_with(&destination_path).unwrap(), vec![]);
    }
}
