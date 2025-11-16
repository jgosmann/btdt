use crate::cache_fixture::CacheFixture;
use biscuit_auth::KeyPair;
use biscuit_auth::macros::biscuit;
use btdt::test_util::fs_spec::{DirSpec, Node};
use btdt_server_lib::test_server::BtdtTestServer;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

pub mod cache_fixture;

#[test]
fn test_cmd() {
    let cache_fixture = CacheFixture::new().unwrap();
    for test_dir in [
        "clean-supports-human-units.in",
        "restore-first-matched-key.in",
        "restore-first-matched-key-comma-separated.in",
        "restore-non-existent-key.in",
        "restore-primary-key.in",
        "restore-success-rc-on-any-key.in",
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

#[test]
fn test_remote_roundtrip() {
    let key_dir = tempdir().unwrap();
    let key_path = key_dir.path().join("private_key.pem");
    let key_pair = KeyPair::new();
    let mut keyfile = OpenOptions::new()
        .mode(0o600)
        .create_new(true)
        .write(true)
        .open(&key_path)
        .unwrap();
    keyfile
        .write_all(key_pair.to_private_key_pem().unwrap().as_bytes())
        .unwrap();

    let token = biscuit!("").build(&key_pair).unwrap();
    let token_path = key_dir.path().join("token-file");
    let mut token_file = OpenOptions::new()
        .mode(0o600)
        .create_new(true)
        .write(true)
        .open(&token_path)
        .unwrap();
    token_file
        .write_all(token.to_base64().unwrap().as_bytes())
        .unwrap();

    let server = BtdtTestServer::new(&BTreeMap::from([(
        "BTDT_AUTH_PRIVATE_KEY".into(),
        key_path.to_str().unwrap().to_string(),
    )]))
    .wait_until_ready()
    .unwrap();
    let cache_url = server.base_url().join("test-cache").unwrap();

    let tempdir = tempdir().unwrap();
    let source_path = tempdir.path().join("source-root");
    let destination_paths = [
        tempdir.path().join("destination-root-0"),
        tempdir.path().join("destination-root-1"),
        tempdir.path().join("destination-root-2"),
    ];

    let spec = DirSpec::create_unix_fixture();
    spec.create(source_path.as_ref()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_btdt"))
        .arg("store")
        .arg("--cache")
        .arg(cache_url.as_str())
        .arg("--auth-token-file")
        .arg(&token_path)
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
            .arg(cache_url.as_str())
            .arg("--auth-token-file")
            .arg(&token_path)
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
