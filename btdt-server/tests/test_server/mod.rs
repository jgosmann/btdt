use reqwest::blocking::{Client, RequestBuilder};
use reqwest::Url;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct BtdtTestServer {
    process: Child,
    client: Client,
    base_url: Url,
}

impl Default for BtdtTestServer {
    fn default() -> Self {
        Self::new(&BTreeMap::default())
    }
}

impl BtdtTestServer {
    pub fn new(env: &BTreeMap<String, String>) -> Self {
        static BIND_ADDR: &str = "127.0.0.1:8747";
        let mut command = Command::new(env!("CARGO_BIN_EXE_btdt-server"));
        command.env("BTDT_BIND_ADDRS", BIND_ADDR);
        for (key, value) in env {
            command.env(key, value);
        }
        let process = command
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start btdt-server");
        Self {
            process,
            client: Client::new(),
            base_url: Url::parse(&format!("http://{BIND_ADDR}"))
                .expect("bind address did not form a valid URL"),
        }
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
        let url = self.base_url.join(path).expect("Invalid path");
        self.client.get(url)
    }

    pub fn is_ready(&self) -> bool {
        self.get("/api/health")
            .send()
            .map_or(false, |r| r.error_for_status().is_ok())
    }

    pub fn wait_until_ready(self) -> Result<Self, WaitTimeout> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self.is_ready() {
                return Ok(self);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(WaitTimeout)
    }
}

#[derive(Debug)]
pub struct WaitTimeout;

impl Display for WaitTimeout {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wait timeout exceeded")
    }
}

impl std::error::Error for WaitTimeout {}
