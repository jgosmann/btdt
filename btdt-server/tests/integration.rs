use crate::test_server::BtdtTestServer;
use btdt::cache::remote::RemoteCache;
use btdt::cache::remote::http::HttpClient;
use btdt::pipeline::Pipeline;
use serial_test::serial;
use std::fs;
use tempfile::tempdir;

mod test_server;

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
    let server = BtdtTestServer::default().wait_until_ready().unwrap();
    let mut client = Pipeline::new(
        RemoteCache::new(
            server.base_url(),
            "test-cache",
            HttpClient::default().unwrap(),
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
