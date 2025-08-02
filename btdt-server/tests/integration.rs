use reqwest::blocking::{Client, RequestBuilder};
use reqwest::Url;
use std::fmt::{Debug, Display, Formatter};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct BtdtTestServer {
    process: Child,
    client: Client,
}

impl BtdtTestServer {
    pub fn new() -> Self {
        let process = Command::new(env!("CARGO_BIN_EXE_btdt-server"))
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start btdt-server");
        Self {
            process,
            client: Client::new(),
        }
    }

    pub fn new_wait_until_ready() -> Self {
        let server = Self::new();
        server
            .wait_until_ready()
            .expect("btdt-server did not become ready");
        server
    }
}

impl Drop for BtdtTestServer {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            eprintln!("Failed to kill btdt-server: {}", e);
        }
    }
}

impl BtdtTestServer {
    pub fn get(&self, path: &str) -> RequestBuilder {
        let url = Url::parse("http://127.0.0.1:8707")
            .unwrap()
            .join(path)
            .expect("Invalid path");
        self.client.get(url)
    }

    pub fn is_ready(&self) -> bool {
        self.get("/api/health")
            .send()
            .map_or(false, |r| r.error_for_status().is_ok())
    }

    pub fn wait_until_ready(&self) -> Result<(), WaitTimeout> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self.is_ready() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(WaitTimeout)
    }
}

#[derive(Debug)]
struct WaitTimeout;

impl Display for WaitTimeout {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wait timeout exceeded")
    }
}

impl std::error::Error for WaitTimeout {}

#[test]
fn test_health_endpoint() {
    let server = BtdtTestServer::new_wait_until_ready();
    let response = server.get("/api/health").send().unwrap();
    assert!(
        response.status().is_success(),
        "unexpected status: {}",
        response.status()
    );
}
