use crate::app::Options;
use crate::config::BtdtServerConfig;
use poem::listener::{BoxListener, Listener};
use poem::{listener::TcpListener, Server};

mod app;
mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = BtdtServerConfig::load()?;
    let listener: BoxListener = settings
        .bind_addrs
        .iter()
        .cloned()
        .map(|addr| {
            println!("Listening on http://{addr}");
            TcpListener::bind(addr).boxed()
        })
        .reduce(|a, b| a.combine(b).boxed())
        .ok_or("No bind addresses provided")?;
    Server::new(listener)
        .run(app::create_route(
            Options::builder()
                .enable_api_docs(settings.enable_api_docs)
                .build(),
        ))
        .await?;
    Ok(())
}
