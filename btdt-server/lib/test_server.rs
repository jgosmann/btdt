//! Test server utilities for btdt-server.

use reqwest::blocking::{Client, RequestBuilder};
use reqwest::{Certificate, Url};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};
use std::{env, fs, io};
use tempfile::{NamedTempFile, TempDir, tempdir};

/// TLS leaf certificate and key for testing purposes.
#[allow(unused)]
pub static CERTIFICATE_PKCS12: &[u8] = include_bytes!("../../tls/leaf.p12");
/// TLS CA certificate for testing purposes.
pub static CERTIFICATE_PEM: &[u8] = include_bytes!("../../tls/ca.pem");

/// A test server instance for btdt-server.
pub struct BtdtTestServer {
    _config_file: NamedTempFile,
    _private_key_dir: Option<TempDir>,
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
    fn target_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target-test")
    }

    /// Build the btdt-server binary in test profile.
    pub fn build() {
        let mut build_command = Command::new("cargo");
        build_command.args(&[
            "build",
            "--profile",
            "test",
            "--package",
            "btdt-server",
            "--bin",
            "btdt-server",
            "--target-dir",
            Self::target_dir().to_str().unwrap(),
        ]);
        let mut process = build_command.spawn().expect("failed to build btdt-server");
        if !process.wait().unwrap().success() {
            panic!("failed to build btdt-server");
        }
    }

    /// Run the btdt-server health-check command.
    pub fn run_health_check(base_url: &str, root_cert: Option<&str>) -> Child {
        Self::build();
        let target_dir = Self::target_dir();
        let mut command = Command::new("cargo");
        let mut args = vec![
            "run",
            "--profile",
            "test",
            "--package",
            "btdt-server",
            "--bin",
            "btdt-server",
            "--target-dir",
            target_dir.to_str().unwrap(),
            "--",
            "health-check",
        ];
        if let Some(root_cert) = root_cert {
            args.push("--root-cert");
            args.push(root_cert);
        }
        args.push(base_url);
        command.args(&args);
        command
            .spawn()
            .expect("failed to start btdt-server health-check")
    }

    /// Create and start a new btdt-server test instance.
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

        Self::build();

        let target_dir = Self::target_dir();
        static BIND_ADDR: &str = "127.0.0.1:8707";
        let mut command = Command::new("cargo");
        command.args(&[
            "run",
            "--profile",
            "test",
            "--package",
            "btdt-server",
            "--bin",
            "btdt-server",
            "--target-dir",
            target_dir.to_str().unwrap(),
        ]);
        command.env("CARGO_TARGET_DIR", "foo");
        command.env("BTDT_BIND_ADDRS", BIND_ADDR);
        command.env("BTDT_SERVER_CONFIG_FILE", config_file.path());
        for (key, value) in env {
            command.env(key, value);
        }

        let private_key_dir = if !env.contains_key("BTDT_AUTH_PRIVATE_KEY") {
            let private_key_dir = tempdir().unwrap();
            command.env(
                "BTDT_AUTH_PRIVATE_KEY",
                private_key_dir.path().join("auth-private-key"),
            );
            Some(private_key_dir)
        } else {
            None
        };

        let process = command.spawn().expect("failed to start btdt-server");
        let tls_enabled = env.contains_key("BTDT_TLS_KEYSTORE");
        Self {
            _config_file: config_file,
            _private_key_dir: private_key_dir,
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
    /// Get the base URL of the test server.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Create a GET request to the specified path.
    pub fn get(&self, path: &str) -> RequestBuilder {
        let url = self.base_url.join(path).expect("Invalid path");
        self.client.get(url)
    }

    /// Check if the server is ready by querying the health endpoint.
    pub fn is_ready(&self) -> bool {
        self.get("/api/health")
            .send()
            .map_or(false, |r| r.error_for_status().is_ok())
    }

    /// Wait until the server is ready or timeout after 5 seconds.
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

    /// Wait for the server process to shut down, with a timeout of 60 seconds.
    pub fn wait_for_shutdown(mut self) -> Result<io::Result<ExitStatus>, WaitTimeout> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            if let Some(status) = self.process.try_wait().transpose() {
                return Ok(status);
            }
        }
        Err(WaitTimeout)
    }
}

/// Error indicating that a wait operation has timed out.
#[derive(Debug)]
pub struct WaitTimeout;

impl Display for WaitTimeout {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wait timeout exceeded")
    }
}

impl std::error::Error for WaitTimeout {}
