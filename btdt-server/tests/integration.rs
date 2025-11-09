use biscuit_auth::macros::biscuit;
use biscuit_auth::{KeyPair, UnverifiedBiscuit};
use btdt::cache::remote::RemoteCache;
use btdt::cache::remote::http::HttpClient;
use btdt::pipeline::Pipeline;
use btdt_server_lib::test_server::BtdtTestServer;
use serial_test::serial;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use tempfile::tempdir;

#[test]
#[serial]
fn test_health_endpoint() {
    let server = BtdtTestServer::default().wait_until_ready().unwrap();
    let response = server.get("/api/health").send().unwrap();
    assert!(
        response.status().is_success(),
        "unexpected status: {}",
        response.status()
    );
}

#[test]
#[serial]
fn test_roundtrip() {
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
    let token =
        UnverifiedBiscuit::from(&biscuit!("").build(&key_pair).unwrap().to_vec().unwrap()).unwrap();

    let server = BtdtTestServer::new(&BTreeMap::from([(
        "BTDT_AUTH_PRIVATE_KEY".into(),
        key_path.to_str().unwrap().to_string(),
    )]))
    .wait_until_ready()
    .unwrap();
    let mut client = Pipeline::new(
        RemoteCache::new(
            server.base_url(),
            "test-cache",
            HttpClient::default().unwrap(),
            token,
        )
        .unwrap(),
    );

    let tempdir = tempdir().unwrap();
    let source_path = tempdir.path().join("source-root");
    fs::create_dir(&source_path).unwrap();
    fs::write(source_path.join("file.txt"), "Hello, world!").unwrap();

    client.store(&["key1", "key2"], &source_path).unwrap();

    let destination_path1 = tempdir.path().join("destination-root-1");
    client
        .restore(&["non-existent", "key1"], &destination_path1)
        .unwrap();

    let destination_path2 = tempdir.path().join("destination-root-2");
    client.restore(&["key2"], &destination_path2).unwrap();

    assert_eq!(
        fs::read_to_string(destination_path1.join("file.txt")).unwrap(),
        "Hello, world!"
    );
    assert_eq!(
        fs::read_to_string(destination_path2.join("file.txt")).unwrap(),
        "Hello, world!"
    );
}
