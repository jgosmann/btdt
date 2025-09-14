use crate::app::Options;
use crate::config::BtdtServerConfig;
use chrono::Local;
use data_encoding::BASE64;
use poem::listener::{BoxListener, Listener, NativeTlsConfig};
use poem::{Endpoint, EndpointExt, Middleware, Request, Server, listener::TcpListener};
use std::borrow::Cow;
use std::error::Error;
use std::fs::File;
use std::io::Read;

mod app;
mod config;

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
    type Output = E::Output;

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
            .and_then(|auth| BASE64.decode(auth.as_bytes()).ok())
            .and_then(|decoded_auth| {
                decoded_auth
                    .split(|&c| c == b':')
                    .next()
                    .map(|u| Cow::Owned(String::from_utf8_lossy(u).into_owned()))
            })
            .unwrap_or(Cow::Borrowed("-"));
        let time = Local::now().format("%d/%b/%Y:%H:%M:%S %z");
        println!(
            "{remote_addr} - {basic_auth_user} [{time}] \"{method} {original_uri} {version:?}\" - - \"{referer}\" \"{user_agent}\""
        );

        self.ep.call(req).await
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let settings = BtdtServerConfig::load()?;
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
        File::open(settings.tls_keystore)?.read_to_end(&mut cert_buf)?;
        listener = listener
            .native_tls(
                NativeTlsConfig::new()
                    .pkcs12(cert_buf)
                    .password(settings.tls_keystore_password),
            )
            .boxed()
    }

    let protocol = if enable_tls { "https" } else { "http" };
    for addr in &settings.bind_addrs {
        println!("Listening on {protocol}://{addr}");
    }

    Server::new(listener)
        .run(
            app::create_route(
                Options::builder()
                    .enable_api_docs(settings.enable_api_docs)
                    .build(),
                &settings.caches,
            )
            .with(AccessLogMiddleware {})
            .with(ErrorLogMiddleware {}),
        )
        .await?;
    Ok(())
}
