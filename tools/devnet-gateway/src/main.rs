use std::net::SocketAddr;
use std::{error::Error, future::Future};

use devnet_gateway::{AppState, account_address, router, run_replica_sync, validator_public_key};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args();
    let _program = arguments.next();
    let command = arguments.next();
    if matches!(command.as_deref(), Some("public-key" | "account-address")) {
        let path = arguments.next().ok_or("missing seed file")?;
        if arguments.next().is_some() {
            return Err("unexpected key command arguments".into());
        }
        let output = if command.as_deref() == Some("public-key") {
            validator_public_key(path)?
        } else {
            account_address(path)?
        };
        println!("{output}");
        return Ok(());
    }
    if command.is_some() {
        return Err("unexpected command".into());
    }
    let address = std::env::var("DEVNET_BIND")
        .unwrap_or_else(|_| "127.0.0.1:18080".to_owned())
        .parse::<SocketAddr>()?;
    let data_directory =
        std::env::var("DEVNET_DATA_DIR").unwrap_or_else(|_| ".devnet/gateway".to_owned());
    let state = AppState::open_configured(data_directory)?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(serve(address, state.clone()))?;
    drop(runtime);
    drop(state);
    Ok(())
}

async fn serve(address: SocketAddr, state: AppState) -> Result<(), Box<dyn Error>> {
    if let Ok(upstream) = std::env::var("DEVNET_SYNC_UPSTREAM")
        && !upstream.trim().is_empty()
    {
        let sync_state = state.clone();
        spawn_detached(async move {
            let _result = run_replica_sync(sync_state, upstream).await;
        });
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn spawn_detached(task: impl Future<Output = ()> + Send + 'static) {
    drop(tokio::spawn(task));
}
