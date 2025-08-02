use crate::test_server::BtdtTestServer;
use serial_test::serial;

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
