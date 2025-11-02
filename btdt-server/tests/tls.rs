use btdt_server_lib::test_server::{BtdtTestServer, CERTIFICATE_PKCS12};
use serial_test::serial;
use std::collections::BTreeMap;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
#[serial]
fn test_tls_connection() {
    let mut tmp_cert_file = NamedTempFile::new().unwrap();
    tmp_cert_file.write_all(CERTIFICATE_PKCS12).unwrap();
    let env = BTreeMap::from([
        (
            "BTDT_TLS_KEYSTORE".to_string(),
            tmp_cert_file.path().to_str().unwrap().to_string(),
        ),
        (
            "BTDT_TLS_KEYSTORE_PASSWORD".to_string(),
            "password".to_string(),
        ),
    ]);
    let server = BtdtTestServer::new(&env).wait_until_ready().unwrap();
    let response = server.get("/api/health").send().unwrap();
    assert!(response.status().is_success());
}
