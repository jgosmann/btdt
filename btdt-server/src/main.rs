use crate::app::Options;
use crate::config::BtdtServerConfig;
use poem::listener::{BoxListener, Listener, NativeTlsConfig};
use poem::{listener::TcpListener, Server};
use std::fs::File;
use std::io::Read;

mod app;
mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        .run(app::create_route(
            Options::builder()
                .enable_api_docs(settings.enable_api_docs)
                .build(),
        ))
        .await?;
    Ok(())
}
