use std::{fs, path::Path};

use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{LedgerSnapshot, NetworkId};
use ledger_runtime_core::{LedgerBlockExecutor, StateCommitmentPolicy};
use merchant_checkout_core::{Checkout, CheckoutDefinition, CheckoutError, CheckoutId};
use merchant_checkout_lmdb::{
    LmdbMerchantCheckoutStore, MerchantCheckoutStoreError, MerchantCheckoutStoreOptions,
};
use network_config_auth_ed25519::Ed25519NetworkConfigVerifier;
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, SignedNetworkConfig,
    VerifiedNetworkConfig,
};
use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexWriteOutcome};
use receipt_index_lmdb::{LmdbReceiptIndex, ReceiptIndexStoreOptions};
use receipt_rebuild_core::rebuild_receipt_index;
use state_sync_core::{FinalizedCheckpoint, ManifestId, SyncCompletion};
use tail_state_store_lmdb::{
    CommitOutcome, LmdbStateStoreError, LmdbStoreOptions, LmdbTailStateStore, StoredFinalizedBlock,
    StoredTailState,
};
use tail_sync_core::VerifiedTailBlock;
use transaction_auth_ed25519::Ed25519Verifier;

use crate::{DevnetError, checkout::DEVNET_TENANT};

const CONFIGURATION_KEY: [u8; 32] = [59; 32];
const GENESIS_MANIFEST: [u8; 32] = [60; 32];
// The hybrid lottery load gate retains full finalized payload history. The
// generic 256 MiB LMDB default is exhausted after roughly 33k one-operation
// blocks, so the long-running local devnet needs explicit capacity. LMDB
// reserves address space up front and grows the data file as pages are used;
// reopening an existing environment with a larger map preserves its data.
const LEDGER_MAP_SIZE: usize = 4 * 1024 * 1024 * 1024;
const DERIVED_MAP_SIZE: usize = 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct DevnetPersistence {
    ledger: LmdbTailStateStore,
    receipts: LmdbReceiptIndex,
    checkouts: LmdbMerchantCheckoutStore,
}

impl DevnetPersistence {
    pub fn open(
        root: impl AsRef<Path>,
        network: NetworkId,
        genesis: FinalizedCheckpoint,
        snapshot: &LedgerSnapshot,
        executor: &LedgerBlockExecutor<Ed25519Verifier>,
    ) -> Result<Self, DevnetError> {
        let root = prepare_root(root.as_ref())?;
        let config = verified_config(network, genesis)?;
        let ledger = LmdbTailStateStore::open_ledger(
            root.join("ledger"),
            LmdbStoreOptions {
                map_size: LEDGER_MAP_SIZE,
                ..LmdbStoreOptions::default()
            },
            config,
        )
        .map_err(map_store_error)?;
        match ledger.current_ledger() {
            Ok(_) => {}
            Err(LmdbStateStoreError::NotInitialized) => ledger
                .initialize_ledger_from_snapshot(
                    SyncCompletion {
                        manifest: ManifestId::new(GENESIS_MANIFEST),
                        checkpoint: genesis,
                    },
                    snapshot,
                )
                .map_err(map_store_error)?,
            Err(error) => return Err(map_store_error(error)),
        }
        let receipts = LmdbReceiptIndex::open(
            root.join("receipts"),
            network,
            ReceiptIndexStoreOptions {
                map_size: DERIVED_MAP_SIZE,
                ..ReceiptIndexStoreOptions::default()
            },
        )
        .map_err(|_| DevnetError::StateUnavailable)?;
        let checkouts = LmdbMerchantCheckoutStore::open(
            root.join("checkouts"),
            network,
            MerchantCheckoutStoreOptions {
                map_size: DERIVED_MAP_SIZE,
                ..MerchantCheckoutStoreOptions::default()
            },
        )
        .map_err(map_checkout_error)?;
        let persistence = Self {
            ledger,
            receipts,
            checkouts,
        };
        persistence.rebuild_receipts(executor)?;
        Ok(persistence)
    }

    pub fn current(&self) -> Result<StoredTailState, DevnetError> {
        self.ledger.current().map_err(map_store_error)
    }

    pub fn recovery_base(&self) -> Result<StoredTailState, DevnetError> {
        self.ledger.recovery_base().map_err(map_store_error)
    }

    pub fn finalized_block(&self, height: u64) -> Result<StoredFinalizedBlock, DevnetError> {
        self.ledger.finalized_block(height).map_err(map_store_error)
    }

    pub fn commit_ledger(&self, block: &VerifiedTailBlock) -> Result<(), DevnetError> {
        match self
            .ledger
            .commit_verified_ledger(block)
            .map_err(map_store_error)?
        {
            CommitOutcome::Committed => Ok(()),
            CommitOutcome::ExistingSame => Err(DevnetError::InternalInvariant),
        }
    }

    pub fn commit_receipts(&self, block: &FinalizedReceiptBlock) -> Result<(), DevnetError> {
        match self
            .receipts
            .commit_finalized(block)
            .map_err(|_| DevnetError::StateUnavailable)?
        {
            ReceiptIndexWriteOutcome::Committed | ReceiptIndexWriteOutcome::ExistingSame => Ok(()),
        }
    }

    pub fn rebuild_receipts(
        &self,
        executor: &LedgerBlockExecutor<Ed25519Verifier>,
    ) -> Result<(), DevnetError> {
        rebuild_receipt_index(&self.ledger, &self.receipts, executor)
            .map(|_| ())
            .map_err(|_| DevnetError::StateUnavailable)
    }

    pub fn create_checkout(
        &self,
        definition: CheckoutDefinition,
        now_ms: u64,
    ) -> Result<Checkout, DevnetError> {
        self.checkouts
            .create(definition, now_ms)
            .map(|mutation| mutation.checkout)
            .map_err(map_checkout_error)
    }

    pub fn checkout(&self, id: CheckoutId) -> Result<Checkout, DevnetError> {
        self.find_checkout(id)?.ok_or(DevnetError::CheckoutNotFound)
    }

    pub fn find_checkout(&self, id: CheckoutId) -> Result<Option<Checkout>, DevnetError> {
        self.checkouts
            .get(DEVNET_TENANT, id)
            .map_err(map_checkout_error)
    }

    pub fn claim_checkout(
        &self,
        id: CheckoutId,
        payer: ledger_core::AccountId,
        fee: u128,
        now_ms: u64,
    ) -> Result<Checkout, DevnetError> {
        self.checkouts
            .claim(DEVNET_TENANT, id, payer, fee, now_ms)
            .map(|mutation| mutation.checkout)
            .map_err(map_checkout_error)
    }

    pub fn mark_checkout_recorded(&self, id: CheckoutId, now_ms: u64) -> Result<(), DevnetError> {
        self.checkouts
            .mark_gateway_recorded(DEVNET_TENANT, id, now_ms)
            .map(|_| ())
            .map_err(map_checkout_error)
    }
}

fn prepare_root(path: &Path) -> Result<std::path::PathBuf, DevnetError> {
    fs::create_dir_all(path).map_err(|_| DevnetError::StateUnavailable)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| DevnetError::StateUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(DevnetError::StateUnavailable);
    }
    fs::canonicalize(path).map_err(|_| DevnetError::StateUnavailable)
}

fn verified_config(
    network: NetworkId,
    genesis: FinalizedCheckpoint,
) -> Result<VerifiedNetworkConfig, DevnetError> {
    let key = SigningKey::from_bytes(&CONFIGURATION_KEY);
    let public_key = ConfigurationPublicKey::new(key.verifying_key().to_bytes());
    let config = NetworkConfig::new(
        network,
        genesis,
        1,
        public_key,
        StateCommitmentPolicy::default(),
    )
    .map_err(|_| DevnetError::InternalInvariant)?;
    SignedNetworkConfig::new(
        config,
        ConfigurationSignature::new(key.sign(&config.signing_message()).to_bytes()),
    )
    .verify(network, public_key, &Ed25519NetworkConfigVerifier)
    .map_err(|_| DevnetError::InternalInvariant)
}

const fn map_store_error(error: LmdbStateStoreError) -> DevnetError {
    match error {
        LmdbStateStoreError::MapFull | LmdbStateStoreError::Database => {
            DevnetError::StateUnavailable
        }
        _ => DevnetError::InternalInvariant,
    }
}

const fn map_checkout_error(error: MerchantCheckoutStoreError) -> DevnetError {
    match error {
        MerchantCheckoutStoreError::CheckoutNotFound => DevnetError::CheckoutNotFound,
        MerchantCheckoutStoreError::Domain(
            CheckoutError::DefinitionConflict | CheckoutError::ClaimConflict,
        ) => DevnetError::CheckoutConflict,
        MerchantCheckoutStoreError::Domain(
            CheckoutError::ZeroAmount
            | CheckoutError::InvalidExpiry
            | CheckoutError::InvalidValidity
            | CheckoutError::InvalidFeePolicy
            | CheckoutError::PayerIsMerchant
            | CheckoutError::FeeTooHigh
            | CheckoutError::Expired,
        ) => DevnetError::InvalidCheckout,
        MerchantCheckoutStoreError::Database | MerchantCheckoutStoreError::MapFull => {
            DevnetError::StateUnavailable
        }
        MerchantCheckoutStoreError::Domain(
            CheckoutError::InvalidTransition
            | CheckoutError::TimestampRegression
            | CheckoutError::CorruptState,
        )
        | MerchantCheckoutStoreError::UnsafePath
        | MerchantCheckoutStoreError::MapSizeTooSmall
        | MerchantCheckoutStoreError::WrongNetwork
        | MerchantCheckoutStoreError::CorruptRecord => DevnetError::InternalInvariant,
    }
}
