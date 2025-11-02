use reqwest::blocking::{Client, RequestBuilder};
use reqwest::{Certificate, Url};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[allow(unused)]
pub static CERTIFICATE_PKCS12: &[u8] = include_bytes!("../../../tls/leaf.p12");
pub static CERTIFICATE_PEM: &[u8] = include_bytes!("../../../tls/ca.pem");

pub struct BtdtTestServer {
    _config_file: NamedTempFile,
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
        let config_file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        fs::write(
            config_file.path(),
            "\
                [caches]\n\
                test-cache = { type = 'InMemory' }\
        ",
        )
        .unwrap();

        static BIND_ADDR: &str = "127.0.0.1:8707";
        let mut command = Command::new(env!("CARGO_BIN_EXE_btdt-server"));
        command.env("BTDT_BIND_ADDRS", BIND_ADDR);
        command.env("BTDT_SERVER_CONFIG_FILE", config_file.path());
        for (key, value) in env {
            command.env(key, value);
        }
        let process = command.spawn().expect("failed to start btdt-server");
        let tls_enabled = env.contains_key("BTDT_TLS_KEYSTORE");
        Self {
            _config_file: config_file,
            process,
            client: Client::builder()
                .add_root_certificate(Certificate::from_pem(CERTIFICATE_PEM).unwrap())
                .use_rustls_tls()
                .build()
                .unwrap(),
            base_url: Url::parse(&format!(
                "http{}://{BIND_ADDR}",
                if tls_enabled { "s" } else { "" }
            ))
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
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

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
