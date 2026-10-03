use std::error::Error;

use payrail_node::{NodeConfig, NodeState, UpstreamClient, router, run_sync_loop};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig::from_environment()?;
    let upstream = UpstreamClient::new(config.upstreams)?;
    let state = NodeState::open(
        config.data_directory,
        upstream,
        &config.validator_public_keys,
    )?;
    let sync_state = state.clone();
    let interval = config.sync_interval;
    let _sync_task = tokio::spawn(async move {
        run_sync_loop(sync_state, interval).await;
    });
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _signal = tokio::signal::ctrl_c().await;
}
