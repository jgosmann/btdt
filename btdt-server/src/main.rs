use crate::app::Options;
use crate::config::{BtdtServerConfig, CleanupConfig};
use crate::storage::StorageHandle;
use biscuit_auth::KeyPair;
use btdt::cache::cache_dispatcher::CacheDispatcher;
use btdt::error::IoPathResult;
use btdt::util::http::{HttpClient, Url};
use btdt::util::humanbytes;
use chrono::{Local, TimeDelta};
use clap::{Parser, Subcommand};
use data_encoding::BASE64;
use poem::listener::{BoxListener, Listener, NativeTlsConfig};
use poem::{
    Endpoint, EndpointExt, IntoResponse, Middleware, Request, Response, Server,
    listener::TcpListener,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, park_timeout};
use std::time::{Duration, Instant};
use std::{env, fs, thread};
use tokio::select;
use tokio::signal::unix::SignalKind;
use zeroize::Zeroizing;

mod app;
mod config;
mod storage;

struct AccessLogMiddleware {}

struct AccessLogMiddlewareImpl<E: Endpoint> {
    ep: E,
}

impl<E: Endpoint> Middleware<E> for AccessLogMiddleware {
    type Output = AccessLogMiddlewareImpl<E>;

    fn transform(&self, ep: E) -> Self::Output {
        AccessLogMiddlewareImpl { ep }
    }
}

impl<E: Endpoint> Endpoint for AccessLogMiddlewareImpl<E> {
    type Output = Response;

    async fn call(&self, req: Request) -> poem::Result<Self::Output> {
        let version = req.version();
        let method = req.method();
        let original_uri = req.original_uri();
        let remote_addr = req
            .remote_addr()
            .as_socket_addr()
            .map(|addr| Cow::Owned(addr.ip().to_string()))
            .unwrap_or(Cow::Borrowed("-"));
        let referer = req
            .headers()
            .get("Referer")
            .and_then(|v| v.to_str().map(|r| Cow::Owned(r.replace('"', "\\\""))).ok())
            .unwrap_or(Cow::Borrowed("-"));
        let user_agent = req
            .headers()
            .get("User-Agent")
            .and_then(|v| {
                v.to_str()
                    .map(|ua| Cow::Owned(ua.replace('"', "\\\"")))
                    .ok()
            })
            .unwrap_or(Cow::Borrowed("-"));
        let basic_auth_user = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|auth| auth.split_once(' '))
            .and_then(|(scheme, credential)| {
                if scheme.eq_ignore_ascii_case("basic") {
                    Some(credential)
                } else {
                    None
                }
            })
            .and_then(|auth| BASE64.decode(auth.as_bytes()).ok())
            .and_then(|decoded_auth| {
                decoded_auth
                    .split(|&c| c == b':')
                    .next()
                    .map(|u| Cow::Owned(String::from_utf8_lossy(u).into_owned()))
            })
            .unwrap_or(Cow::Borrowed("-"));
        let time = Local::now().format("%d/%b/%Y:%H:%M:%S %z");

        let log_start = format!(
            "{remote_addr} - {basic_auth_user} [{time}] \"{method} {original_uri} {version:?}\""
        );
        let log_end = format!("\"{referer}\" \"{user_agent}\"");

        let result = self.ep.call(req).await.map(|res| res.into_response());
        let status = result
            .as_ref()
            .map(|res| Cow::Owned(res.status().as_u16().to_string()))
            .or_else(|err| {
                Result::<_, Infallible>::Ok(Cow::Owned(err.status().as_u16().to_string()))
            })
            .unwrap_or(Cow::Borrowed("-"));

        println!("{log_start} {status} - {log_end}");
        result
    }
}

struct ErrorLogMiddleware {}

struct ErrorLogMiddlewareImpl<E: Endpoint> {
    ep: E,
}

impl<E: Endpoint> Middleware<E> for ErrorLogMiddleware {
    type Output = ErrorLogMiddlewareImpl<E>;

    fn transform(&self, ep: E) -> Self::Output {
        ErrorLogMiddlewareImpl { ep }
    }
}

impl<E: Endpoint> Endpoint for ErrorLogMiddlewareImpl<E> {
    type Output = E::Output;

    async fn call(&self, req: Request) -> poem::Result<Self::Output> {
        let method = req.method().to_string();
        let original_uri = req.original_uri().clone();
        match self.ep.call(req).await {
            Ok(response) => Ok(response),
            Err(mut err) => {
                let source = err.source().unwrap_or(&err);
                eprintln!("Error in request for {method} {original_uri}: {source:?}");
                if err.status().is_server_error() {
                    err.set_error_message("Internal Server Error");
                }
                Err(err)
            }
        }
    }
}

fn load_or_create_auth_keys(private_key_path: &str) -> Result<KeyPair, Box<dyn Error>> {
    let humanize_auth_key_error = |err| format!("BTDT_AUTH_PRIVATE_KEY={private_key_path}: {err}");
    if !fs::exists(private_key_path).map_err(humanize_auth_key_error)? {
        let mut keyfile = OpenOptions::new()
            .mode(0o600)
            .create_new(true)
            .write(true)
            .open(private_key_path)
            .map_err(humanize_auth_key_error)?;
        let key_pair = KeyPair::new();
        keyfile.write_all(key_pair.to_private_key_pem().unwrap().as_bytes())?;
        Ok(key_pair)
    } else {
        let auth_private_key_meta =
            fs::metadata(private_key_path).map_err(humanize_auth_key_error)?;
        if auth_private_key_meta.permissions().mode() & 0o077 != 0 {
            return Err(format!("The private key file {private_key_path} for authentication must not be accessible by group or others. Please set its permission to 0600 or similar.").into());
        };
        let mut keyfile = File::open(private_key_path).map_err(humanize_auth_key_error)?;
        let mut key_pem = Zeroizing::new(String::new());
        keyfile
            .read_to_string(&mut key_pem)
            .map_err(humanize_auth_key_error)?;
        Ok(KeyPair::from_private_key_pem(&key_pem)?)
    }
}

/// btdt-server - cache server for btdt "been there, done that"
#[derive(Parser)]
#[command(version)]
struct CliOpts {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    // Check health of a remote btdt-server instance.
    HealthCheck {
        base_url: Url,

        /// Root certificates (in PEM format) to trust (instead of system's root certificates).
        #[arg(long, env = "BTDT_TRUSTED_ROOT_CERTS", value_delimiter = ',')]
        root_cert: Vec<PathBuf>,
    },
    // Start the btdt-server.
    Start {},
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_opts = CliOpts::parse();
    match cli_opts.command {
        Some(Commands::HealthCheck {
            base_url,
            root_cert,
        }) => {
            let client = if root_cert.is_empty() {
                HttpClient::default()
            } else {
                HttpClient::with_tls_root_cert_paths(&root_cert)
            }?;
            let health_url = base_url.join("/api/health")?;
            let (status, resp) = client.get(&health_url)?.no_body()?.read_status()?;
            if status.is_success() {
                println!("Server is healthy.");
                Ok(())
            } else {
                let body = resp
                    .read_body()
                    .and_then(|mut reader| {
                        let mut buf = String::new();
                        reader.read_to_string(&mut buf)?;
                        Ok(buf)
                    })
                    .unwrap_or_else(|err| err.to_string());
                Err(format!(
                    "Server health check failed with status: {}\n{body}",
                    status.code(),
                )
                .into())
            }
        }
        Some(Commands::Start {}) | None => run_server().await,
    }
}

async fn run_server() -> Result<(), Box<dyn Error>> {
    println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let settings = BtdtServerConfig::load()?;

    let auth_key_pair = load_or_create_auth_keys(&settings.auth_private_key)?;

    let mut listener: BoxListener = settings
        .bind_addrs
        .iter()
        .cloned()
        .map(|addr| TcpListener::bind(addr).boxed())
        .reduce(|a, b| a.combine(b).boxed())
        .ok_or("No bind addresses provided")?;

    let enable_tls = !settings.tls_keystore.is_empty();
    if enable_tls {
        let mut cert_buf = Vec::new();
        File::open(&settings.tls_keystore)?.read_to_end(&mut cert_buf)?;
        listener = listener
            .native_tls(
                NativeTlsConfig::new()
                    .pkcs12(cert_buf)
                    .password(&settings.tls_keystore_password),
            )
            .boxed()
    }

    let storage_locations: BTreeMap<String, StorageHandle> = settings
        .caches
        .iter()
        .map(|(key, cache_config)| (key.clone(), StorageHandle::from(cache_config)))
        .collect();

    let cleanup_caches = storage_locations
        .iter()
        .map(|(key, storage_handle)| (key.clone(), storage_handle.to_cache()))
        .collect();
    let cleanup_task = CleanupTask::new(cleanup_caches, &settings.cleanup)?.run();

    let caches: HashMap<String, CacheDispatcher> = storage_locations
        .into_iter()
        .map(|(key, storage_handle)| (key, storage_handle.into_cache()))
        .collect();

    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())?;

    let protocol = if enable_tls { "https" } else { "http" };
    for addr in &settings.bind_addrs {
        println!("Listening on {protocol}://{addr}");
    }

    Server::new(listener)
        .run_with_graceful_shutdown(
            app::create_route(
                Options::builder()
                    .enable_api_docs(settings.enable_api_docs)
                    .build(),
                caches,
                auth_key_pair,
            )
            .with(AccessLogMiddleware {})
            .with(ErrorLogMiddleware {}),
            async {
                select! {
                    _ = sigint.recv() => {},
                    _ = sigterm.recv() => {},
                }
                println!("Shutting down...");
            },
            None,
        )
        .await?;
    cleanup_task.abort();
    cleanup_task.join().map_err(|_| "Cleanup thread failed.")?;
    Ok(())
}

struct CleanupTask {
    caches: HashMap<String, CacheDispatcher>,
    cleanup_interval: Duration,
    cache_expiration: TimeDelta,
    max_cache_size: u64,
}

impl CleanupTask {
    pub fn new(
        caches: HashMap<String, CacheDispatcher>,
        settings: &CleanupConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let cleanup_interval = humantime::parse_duration(&settings.interval)?;
        let cache_expiration =
            TimeDelta::from_std(humantime::parse_duration(&settings.cache_expiration)?)?;
        let max_cache_size = humanbytes::parse_bytes_from_str(&settings.max_cache_size)?;
        Ok(Self {
            caches,
            cleanup_interval,
            cache_expiration,
            max_cache_size,
        })
    }

    pub fn run(mut self) -> CleanupTaskHandle {
        let is_aborted_rx = Arc::new(AtomicBool::new(false));
        let is_aborted_tx = is_aborted_rx.clone();
        let join_handle = thread::Builder::new()
            .name("cleanup".to_string())
            .spawn(move || {
                let mut parked_since = Instant::now();
                loop {
                    if let Some(timeout_remaining) =
                        self.cleanup_interval.checked_sub(parked_since.elapsed())
                    {
                        park_timeout(timeout_remaining);
                    }
                    if is_aborted_rx.load(Ordering::Acquire) {
                        break;
                    }
                    if parked_since.elapsed() < self.cleanup_interval {
                        continue;
                    }
                    for cache in self.caches.values_mut() {
                        if let Err(e) =
                            cache.clean_cache(self.cache_expiration, self.max_cache_size)
                        {
                            eprintln!("Error during periodic cleanup: {e}");
                        }
                    }
                    parked_since = Instant::now();
                }
            })
            .expect("Unable to spawn cleanup thread");
        CleanupTaskHandle {
            is_aborted: is_aborted_tx,
            join_handle,
        }
    }
}

trait Clean {
    fn clean_cache(&mut self, cache_expiration: TimeDelta, max_cache_size: u64)
    -> IoPathResult<()>;
}

impl Clean for CacheDispatcher {
    fn clean_cache(
        &mut self,
        cache_expiration: TimeDelta,
        max_cache_size: u64,
    ) -> IoPathResult<()> {
        match self {
            CacheDispatcher::InMemory(cache) => {
                cache.clean(Some(cache_expiration), Some(max_cache_size))
            }
            CacheDispatcher::Filesystem(cache) => {
                cache.clean(Some(cache_expiration), Some(max_cache_size))
            }
            CacheDispatcher::Remote(_) => Ok(()),
        }
    }
}

struct CleanupTaskHandle {
    is_aborted: Arc<AtomicBool>,
    join_handle: JoinHandle<()>,
}

impl CleanupTaskHandle {
    fn abort(&self) {
        self.is_aborted.store(true, Ordering::Release);
        self.join_handle.thread().unpark();
    }

    fn join(self) -> Result<(), Box<dyn std::any::Any + Send>> {
        self.join_handle.join()
    }
}
