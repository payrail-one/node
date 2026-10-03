use std::error::Error;
use std::net::SocketAddr;

use devnet_gateway::{AppState, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let address = std::env::var("DEVNET_BIND")
        .unwrap_or_else(|_| "127.0.0.1:18080".to_owned())
        .parse::<SocketAddr>()?;
    let data_directory =
        std::env::var("DEVNET_DATA_DIR").unwrap_or_else(|_| ".devnet/gateway".to_owned());
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(AppState::open(data_directory)?)).await?;
    Ok(())
}
