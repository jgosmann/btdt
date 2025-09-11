use crate::app::Options;
use crate::config::BtdtServerConfig;
use poem::listener::{BoxListener, Listener, NativeTlsConfig};
use poem::{Endpoint, EndpointExt, Middleware, Request, Server, listener::TcpListener};
use std::fs::File;
use std::io::Read;

mod app;
mod config;

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
        match self.ep.call(req).await {
            Ok(response) => Ok(response),
            Err(err) => {
                eprintln!("Error: {:?}", err);
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
            )
            .with(ErrorLogMiddleware {}),
        )
        .await?;
    Ok(())
}
