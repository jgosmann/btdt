use poem::{listener::TcpListener, Server};

mod app;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    Server::new(TcpListener::bind("127.0.0.1:8707"))
        .run(app::create_route())
        .await
}
