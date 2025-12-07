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
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use tempfile::tempdir;

struct BtdtTestServerWithAuthorizedClient {
    _key_dir: tempfile::TempDir,
    _server: BtdtTestServer,
    client: Pipeline<RemoteCache>,
}

impl Default for BtdtTestServerWithAuthorizedClient {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl BtdtTestServerWithAuthorizedClient {
    fn new(mut env: BTreeMap<String, String>) -> Self {
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
            UnverifiedBiscuit::from(&biscuit!("").build(&key_pair).unwrap().to_vec().unwrap())
                .unwrap();

        env.insert(
            "BTDT_AUTH_PRIVATE_KEY".into(),
            key_path.to_str().unwrap().to_string(),
        );
        let server = BtdtTestServer::new(&env).wait_until_ready().unwrap();

        let client = Pipeline::new(
            RemoteCache::new(
                server.base_url().join("api/caches/test-cache").unwrap(),
                HttpClient::default().unwrap(),
                token,
            )
            .unwrap(),
        );

        Self {
            client,
            _key_dir: key_dir,
            _server: server,
        }
    }
}

struct TestData {
    _tempdir: tempfile::TempDir,
    path: PathBuf,
}

impl Default for TestData {
    fn default() -> Self {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("source-root");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("file.txt"), "Hello, world!").unwrap();
        Self {
            _tempdir: tempdir,
            path,
        }
    }
}

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
fn test_health_check_fails_without_running_server() {
    let mut process = BtdtTestServer::run_health_check("http://example.invalid", None);
    assert!(!process.wait().unwrap().success());
}

#[test]
#[serial]
fn test_health_check_succeeds_with_running_server() {
    let server = BtdtTestServer::default().wait_until_ready().unwrap();
    let mut process = BtdtTestServer::run_health_check(server.base_url().as_str(), None);
    assert!(process.wait().unwrap().success());
}

#[test]
#[serial]
fn test_roundtrip() {
    let server_with_client = BtdtTestServerWithAuthorizedClient::default();
    let mut client = server_with_client.client;
    let test_data = TestData::default();

    client.store(&["key1", "key2"], &test_data.path).unwrap();

    let tempdir = tempdir().unwrap();
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

#[test]
#[serial]
fn test_cleanup() {
    let server_with_client = BtdtTestServerWithAuthorizedClient::new(BTreeMap::from([
        ("BTDT_CLEANUP__MAX_CACHE_SIZE".to_string(), "0B".to_string()),
        (
            "BTDT_CLEANUP__CACHE_EXPIRATION".to_string(),
            "1s".to_string(),
        ),
        ("BTDT_CLEANUP__INTERVAL".to_string(), "1s".to_string()),
    ]));
    let mut client = server_with_client.client;
    let test_data = TestData::default();

    client.store(&["key"], &test_data.path).unwrap();
    sleep(Duration::from_secs(2));

    let tempdir = tempdir().unwrap();
    assert_eq!(client.restore(&["key"], &tempdir).unwrap(), None);
}
