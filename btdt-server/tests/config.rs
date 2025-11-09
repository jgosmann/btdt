use biscuit_auth::KeyPair;
use btdt_server_lib::test_server::BtdtTestServer;
use reqwest::StatusCode;
use serial_test::serial;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

#[test]
#[serial]
fn test_api_docs_enabled_by_default() {
    let server = BtdtTestServer::default().wait_until_ready().unwrap();
    let response = server.get("/docs").send().unwrap();
    assert!(response.status().is_success());
}

#[test]
#[serial]
fn test_disable_api_docs() {
    let server = BtdtTestServer::new(&BTreeMap::from([(
        "BTDT_ENABLE_API_DOCS".to_string(),
        "false".to_string(),
    )]))
    .wait_until_ready()
    .unwrap();
    let response = server.get("/docs").send().unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
#[serial]
fn test_will_not_start_with_lenient_permission_on_auth_private_key() {
    let private_key_file = NamedTempFile::new().unwrap();
    std::fs::set_permissions(
        private_key_file.path(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    let server = BtdtTestServer::new(&BTreeMap::from([(
        "BTDT_AUTH_PRIVATE_KEY".to_string(),
        private_key_file
            .path()
            .to_str()
            .expect("path not convertible to str")
            .to_string(),
    )]));

    let result = server.wait_for_shutdown().unwrap().unwrap();
    assert!(result.code().is_some_and(|code| code != 0));
}

#[test]
#[serial]
fn test_creates_private_key_file_with_strict_permissions_if_missing() {
    let temp_dir = tempfile::tempdir().unwrap();
    let private_key_path = temp_dir.path().join("private_key.pem");

    let server = BtdtTestServer::new(&BTreeMap::from([(
        "BTDT_AUTH_PRIVATE_KEY".to_string(),
        private_key_path
            .to_str()
            .expect("path not convertible to str")
            .to_string(),
    )]));
    server.wait_until_ready().unwrap();

    let metadata = std::fs::metadata(&private_key_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "expected private key file to have 0o600 permissions, got {:o}",
        mode
    );

    let mut key_pem = Zeroizing::new(String::new());
    let mut keyfile = File::open(&private_key_path).unwrap();
    keyfile.read_to_string(&mut key_pem).unwrap();
    assert!(KeyPair::from_private_key_pem(&key_pem).is_ok());
}
