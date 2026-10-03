use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use account_address::AddressCodec;
use devnet_gateway::{AppState, DevnetService};
use ed25519_dalek::SigningKey;
use ledger_core::{AccountId, NetworkId};
use payrail_node::{NodeState, UpstreamClient};
use reqwest::Url;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "payrail-node-{name}-{}-{sequence}",
            std::process::id()
        )))
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _result = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn public_node_catches_up_from_a_payrail_upstream() {
    let source_directory = TestDirectory::new("source");
    let replica_directory = TestDirectory::new("replica");
    let key = SigningKey::from_bytes(&[111; 32]);
    let address = AddressCodec::new(NetworkId::new([17; 32]), "paydev")
        .unwrap()
        .encode(AccountId::new(key.verifying_key().to_bytes()))
        .unwrap();
    let mut source = DevnetService::open(&source_directory.0).unwrap();
    source.faucet(&address).unwrap();
    drop(source);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let source_state = AppState::open(&source_directory.0).unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, devnet_gateway::router(source_state))
            .await
            .unwrap();
    });

    let client = UpstreamClient::new(vec![origin]).unwrap();
    let replica = NodeState::open(&replica_directory.0, client).unwrap();
    assert_eq!(replica.synchronize().await.unwrap(), 1);
    assert_eq!(replica.account(&address).unwrap().balance, "100000000");
    let (_, health) = replica.status().unwrap();
    assert!(health.synchronized);
    assert_eq!(health.target_height, Some(1));

    server.abort();
}
