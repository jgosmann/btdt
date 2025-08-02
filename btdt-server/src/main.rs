use config::Config;
use poem::listener::{BoxListener, Listener};
use poem::{listener::TcpListener, Server};

mod app;

#[derive(Clone, Debug, serde::Deserialize)]
struct BtdtServerConfig {
    bind_addrs: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings: BtdtServerConfig = Config::builder()
        .set_default("bind_addrs", vec!["0.0.0.0:8707".to_string()])
        .unwrap()
        .add_source(config::File::with_name("/etc/btdt-server").required(false))
        .add_source(
            config::Environment::with_prefix("BTDT")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("bind_addrs"),
        )
        .build()?
        .try_deserialize()
        .map_err(|err| format!("configuration: {err}"))?;
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
    Server::new(listener).run(app::create_route()).await?;
    Ok(())
}
