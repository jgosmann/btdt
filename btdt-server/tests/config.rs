use crate::test_server::BtdtTestServer;
use reqwest::StatusCode;
use serial_test::serial;
use std::collections::BTreeMap;

mod test_server;

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
