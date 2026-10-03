use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use account_address::AddressCodec;
use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, NetworkId, OperationOutcome,
    SignatureBytes, SignedOperation, Transfer,
};
use ledger_runtime_core::{LedgerBlockCodec, LedgerBlockExecutor, LedgerStateCodec, state_root};
use merchant_checkout_core::CheckoutId;
use receipt_index_core::FinalizedReceiptBlock;
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, StateRoot, ValidatorSetHash,
};
use tail_sync_core::{FinalizedTailBlock, TailSyncSession};
use transaction_auth_ed25519::Ed25519Verifier;
use transaction_protocol::{MAX_ENVELOPE_BYTES, SignedOperationCodec};

use crate::{
    DevnetError,
    checkout::{create_definition, now_ms, parse_id, validate_payment, validate_transfer, view},
    codec::{decode_bounded_hex, encode_hex},
    index::ExplorerIndex,
    model::{
        AccountStateView, CheckoutView, ExplorerOverviewView, FinalizedBlockView,
        FinalizedTransactionView, NetworkAssetView, NetworkStatusView, SubmissionResultView,
    },
    persistence::DevnetPersistence,
};

pub(crate) const NETWORK: NetworkId = NetworkId::new([17; 32]);
pub(crate) const ASSET: AssetId = AssetId::new([34; 32]);
const ADDRESS_PREFIX: &str = "paydev";
const SYMBOL: &str = "TEST";
const DECIMALS: u8 = 6;
const FAUCET_GRANT: u128 = 100_000_000;
const FAUCET_SUPPLY: u128 = 1_000_000_000_000_000;
const RECENT_VALIDITY_WINDOW: u64 = 20;
const DEVNET_FINALITY_PROOF: &[u8] = b"single-node-devnet-finality";

type RebuiltPresentation = (
    ExplorerIndex,
    BTreeSet<AccountId>,
    BTreeMap<CheckoutId, FinalizedTransactionView>,
);

#[derive(Debug)]
pub struct DevnetService {
    address_codec: AddressCodec,
    faucet_key: SigningKey,
    faucet_account: AccountId,
    executor: LedgerBlockExecutor<Ed25519Verifier>,
    persistence: DevnetPersistence,
    state: Vec<u8>,
    checkpoint: FinalizedCheckpoint,
    funded: BTreeSet<AccountId>,
    settled_checkouts: BTreeMap<CheckoutId, FinalizedTransactionView>,
    index: ExplorerIndex,
}

impl DevnetService {
    /// Opens a deterministic single-process ledger with persistent LMDB state.
    ///
    /// This mode is deliberately labelled `single-node-devnet`; it exercises
    /// canonical signatures and the real ledger runtime but makes no BFT or
    /// production-availability claim.
    ///
    /// # Errors
    ///
    /// Fails if deterministic genesis, persistent recovery, derived-index
    /// rebuilding or address configuration violates an invariant.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DevnetError> {
        let faucet_key = SigningKey::from_bytes(&[51; 32]);
        let faucet_account = AccountId::new(faucet_key.verifying_key().to_bytes());
        let registry = AccountId::new([52; 32]);
        let issuer = AccountId::new([53; 32]);
        let mut ledger = Ledger::new(NETWORK, registry);
        ledger
            .register_asset(
                registry,
                AssetDefinition {
                    id: ASSET,
                    symbol: SYMBOL.to_owned(),
                    decimals: DECIMALS,
                    class: AssetClass::NetworkNative,
                    status: AssetStatus::Test,
                    issuer,
                    backing_authority: AccountId::new([54; 32]),
                    freeze_authority: AccountId::new([55; 32]),
                    treasury: AccountId::new([56; 32]),
                    max_supply: Some(FAUCET_SUPPLY),
                    backing_requirement: BackingRequirement::None,
                },
            )
            .map_err(|_| DevnetError::InternalInvariant)?;
        ledger
            .mint(issuer, ASSET, faucet_account, FAUCET_SUPPLY)
            .map_err(|_| DevnetError::InternalInvariant)?;
        let snapshot = ledger.snapshot();
        let genesis_state =
            LedgerStateCodec::encode(&snapshot).map_err(|_| DevnetError::InternalInvariant)?;
        let genesis = FinalizedCheckpoint {
            height: 0,
            block_hash: BlockHash::new([58; 32]),
            state_root: state_root(&genesis_state),
            validator_set_hash: ValidatorSetHash::new([57; 32]),
        };
        let address_codec = AddressCodec::new(NETWORK, ADDRESS_PREFIX)
            .map_err(|_| DevnetError::InternalInvariant)?;
        let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);
        let persistence = DevnetPersistence::open(path, NETWORK, genesis, &snapshot, &executor)?;
        let current = persistence.current()?;
        let (index, funded, settled_checkouts) =
            rebuild_presentation(&persistence, &executor, &address_codec, faucet_account)?;
        Ok(Self {
            address_codec,
            faucet_key,
            faucet_account,
            executor,
            persistence,
            state: current.state,
            checkpoint: current.checkpoint,
            funded,
            settled_checkouts,
            index,
        })
    }

    #[must_use]
    pub fn status(&self) -> NetworkStatusView {
        NetworkStatusView {
            network_id: encode_hex(NETWORK.as_bytes()),
            address_prefix: ADDRESS_PREFIX.to_owned(),
            finalized_height: self.checkpoint.height.to_string(),
            finality_mode: "single-node-devnet",
            asset: NetworkAssetView {
                id: encode_hex(ASSET.as_bytes()),
                symbol: SYMBOL.to_owned(),
                decimals: DECIMALS,
            },
        }
    }

    /// Reads one account from the finalized development ledger.
    ///
    /// # Errors
    ///
    /// Rejects malformed or foreign-network addresses and unavailable state.
    pub fn account(&self, address: &str) -> Result<AccountStateView, DevnetError> {
        let decoded = self
            .address_codec
            .decode(address)
            .map_err(|_| DevnetError::InvalidAddress)?;
        let ledger = self.ledger()?;
        Ok(AccountStateView {
            address: address.to_owned(),
            account_id: encode_hex(decoded.account.as_bytes()),
            nonce: ledger.nonce(decoded.account).to_string(),
            balance: ledger.balance(ASSET, decoded.account).to_string(),
            finalized_height: self.checkpoint.height.to_string(),
        })
    }

    /// Signs and finalizes one bounded test-asset transfer from the faucet.
    ///
    /// # Errors
    ///
    /// Rejects malformed addresses, repeated claims, exhausted funds or any
    /// transaction/runtime/index invariant failure.
    pub fn faucet(&mut self, address: &str) -> Result<SubmissionResultView, DevnetError> {
        let recipient = self
            .address_codec
            .decode(address)
            .map_err(|_| DevnetError::InvalidAddress)?
            .account;
        if self.funded.contains(&recipient) {
            return Err(DevnetError::AccountAlreadyFunded);
        }
        let ledger = self.ledger()?;
        if ledger.balance(ASSET, self.faucet_account) < FAUCET_GRANT {
            return Err(DevnetError::FaucetExhausted);
        }
        let next_height = self
            .checkpoint
            .height
            .checked_add(1)
            .ok_or(DevnetError::InternalInvariant)?;
        let mut idempotency = [0_u8; 32];
        idempotency[..8].copy_from_slice(b"faucet\0\0");
        idempotency[24..].copy_from_slice(&next_height.to_be_bytes());
        let operation = AuthorizedOperation::Transfer(Transfer {
            network: NETWORK,
            idempotency_key: IdempotencyKey::new(idempotency),
            asset: ASSET,
            from: self.faucet_account,
            to: recipient,
            amount: FAUCET_GRANT,
            fee: 0,
            nonce: ledger.nonce(self.faucet_account),
            valid_until_height: next_height
                .checked_add(RECENT_VALIDITY_WINDOW)
                .ok_or(DevnetError::InternalInvariant)?,
        });
        let message = operation
            .authorization_message(AuthorizationRole::Sender)
            .map_err(|_| DevnetError::InternalInvariant)?;
        let signed = SignedOperation {
            operation,
            sender_authorization: Authorization {
                signer: self.faucet_account,
                signature: SignatureBytes::new(self.faucet_key.sign(&message).to_bytes()),
            },
            fee_payer_authorization: None,
        };
        let result = self.finalize(&signed)?;
        self.funded.insert(recipient);
        Ok(result)
    }

    /// Decodes, verifies, executes and indexes one canonical signed envelope.
    ///
    /// # Errors
    ///
    /// Rejects oversized/malformed envelopes, unsupported operations, invalid
    /// signatures or any ledger invariant failure.
    pub fn submit_hex(&mut self, envelope: &str) -> Result<SubmissionResultView, DevnetError> {
        let signed = decode_signed(envelope)?;
        self.finalize(&signed)
    }

    /// Creates an immutable fixed-amount development checkout.
    ///
    /// # Errors
    ///
    /// Rejects invalid addresses, amounts or order references and unavailable
    /// persistent checkout state.
    pub fn create_checkout(
        &self,
        merchant_address: &str,
        amount: &str,
        order_reference: &str,
    ) -> Result<CheckoutView, DevnetError> {
        let merchant = self
            .address_codec
            .decode(merchant_address)
            .map_err(|_| DevnetError::InvalidAddress)?
            .account;
        let created_at = now_ms()?;
        let definition = create_definition(
            merchant,
            amount,
            order_reference,
            self.checkpoint.height,
            created_at,
        )?;
        let checkout = self.persistence.create_checkout(definition, created_at)?;
        self.checkout_view(&checkout, created_at)
    }

    /// Reads one network-bound development checkout.
    ///
    /// # Errors
    ///
    /// Rejects malformed or unknown identifiers and unavailable state.
    pub fn checkout(&self, id: &str) -> Result<CheckoutView, DevnetError> {
        let id = parse_id(id)?;
        let checkout = self.persistence.checkout(id)?;
        self.checkout_view(&checkout, now_ms()?)
    }

    /// Validates, claims and finalizes an exact browser-signed checkout payment.
    ///
    /// # Errors
    ///
    /// Rejects any envelope that differs from the immutable checkout, a
    /// conflicting payer, invalid signature or failed ledger transition.
    pub fn submit_checkout(
        &mut self,
        id: &str,
        envelope: &str,
    ) -> Result<CheckoutView, DevnetError> {
        let id = parse_id(id)?;
        let checkout = self.persistence.checkout(id)?;
        let signed = decode_signed(envelope)?;
        let payer = validate_payment(&checkout, &signed)?;
        let submitted_at = now_ms()?;
        self.persistence
            .claim_checkout(id, payer, 0, submitted_at)?;
        if self.settled_checkouts.contains_key(&id) {
            self.persistence.mark_checkout_recorded(id, submitted_at)?;
            let recorded = self.persistence.checkout(id)?;
            return self.checkout_view(&recorded, submitted_at);
        }
        self.finalize(&signed)?;
        self.persistence.mark_checkout_recorded(id, submitted_at)?;
        let recorded = self.persistence.checkout(id)?;
        self.checkout_view(&recorded, now_ms()?)
    }

    #[must_use]
    pub fn explorer(&self) -> ExplorerOverviewView {
        ExplorerOverviewView {
            status: self.status(),
            blocks: self.index.blocks(),
            transactions: self.index.transactions(),
        }
    }

    fn finalize(&mut self, signed: &SignedOperation) -> Result<SubmissionResultView, DevnetError> {
        let transfer = match &signed.operation {
            AuthorizedOperation::Transfer(transfer) => *transfer,
            _ => return Err(DevnetError::UnsupportedOperation),
        };
        let payload = LedgerBlockCodec::encode(std::slice::from_ref(signed))
            .map_err(|_| DevnetError::InvalidEnvelope)?;
        let executed = self
            .executor
            .execute(self.checkpoint, &self.state, &payload)
            .map_err(|_| DevnetError::InvalidTransaction)?;
        let receipt = executed
            .receipts
            .first()
            .copied()
            .ok_or(DevnetError::InternalInvariant)?;
        let checkpoint = FinalizedCheckpoint {
            height: self
                .checkpoint
                .height
                .checked_add(1)
                .ok_or(DevnetError::InternalInvariant)?,
            block_hash: executed.transition.commitment.block_hash,
            state_root: executed.transition.commitment.state_root,
            validator_set_hash: self.checkpoint.validator_set_hash,
        };
        let transaction = transaction_view(&self.address_codec, transfer, receipt, checkpoint)?;
        let block = block_view(checkpoint, 1);
        let prepared = self.index.prepare_append(
            self.checkpoint,
            checkpoint,
            block.clone(),
            transaction.clone(),
        )?;
        let finalized = FinalizedTailBlock {
            network: NETWORK,
            parent_hash: self.checkpoint.block_hash,
            checkpoint,
            payload: payload.clone(),
            finality_proof: DEVNET_FINALITY_PROOF.to_vec(),
        };
        let transition = executed.transition.clone();
        let (verified, ()) = TailSyncSession::new(NETWORK, self.checkpoint)
            .verify_next_with(finalized, &DevnetFinality, |_previous, _payload| {
                Some((transition, ()))
            })
            .map_err(|_| DevnetError::InternalInvariant)?;
        let receipt_block = FinalizedReceiptBlock {
            network: NETWORK,
            previous: self.checkpoint,
            checkpoint,
            receipts: executed.receipts,
        };
        self.persistence.commit_ledger(&verified)?;
        self.state = verified.state().to_vec();
        self.checkpoint = checkpoint;
        if self.persistence.commit_receipts(&receipt_block).is_err() {
            self.persistence.rebuild_receipts(&self.executor)?;
        }
        self.index.commit(prepared)?;
        self.record_checkout_settlement(transfer, transaction.clone())?;
        Ok(SubmissionResultView {
            transaction,
            checkpoint: block,
        })
    }

    fn ledger(&self) -> Result<Ledger, DevnetError> {
        let snapshot = LedgerStateCodec::decode(NETWORK, &self.state)
            .map_err(|_| DevnetError::StateUnavailable)?;
        Ledger::from_snapshot(NETWORK, snapshot).map_err(|_| DevnetError::StateUnavailable)
    }

    fn checkout_view(
        &self,
        checkout: &merchant_checkout_core::Checkout,
        current_time_ms: u64,
    ) -> Result<CheckoutView, DevnetError> {
        view(
            checkout,
            &self.address_codec,
            self.status().asset,
            self.settled_checkouts.get(&checkout.id()).cloned(),
            current_time_ms,
        )
    }

    fn record_checkout_settlement(
        &mut self,
        transfer: Transfer,
        transaction: FinalizedTransactionView,
    ) -> Result<(), DevnetError> {
        let id = CheckoutId::new(*transfer.idempotency_key.as_bytes());
        let Some(checkout) = self.persistence.find_checkout(id)? else {
            return Ok(());
        };
        if validate_transfer(&checkout, transfer).is_ok() {
            self.settled_checkouts.insert(id, transaction);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct DevnetFinality;

impl FinalityProofVerifier for DevnetFinality {
    fn verify(&self, network: NetworkId, checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        network == NETWORK
            && checkpoint.validator_set_hash == ValidatorSetHash::new([57; 32])
            && proof == DEVNET_FINALITY_PROOF
    }
}

fn rebuild_presentation(
    persistence: &DevnetPersistence,
    executor: &LedgerBlockExecutor<Ed25519Verifier>,
    addresses: &AddressCodec,
    faucet: AccountId,
) -> Result<RebuiltPresentation, DevnetError> {
    let base = persistence.recovery_base()?;
    let latest = persistence.current()?;
    let mut previous = base.checkpoint;
    let mut state = base.state;
    let mut index = ExplorerIndex::new(previous, block_view(previous, 0));
    let mut funded = BTreeSet::new();
    let mut settled_checkouts = BTreeMap::new();
    while previous.height < latest.checkpoint.height {
        let height = previous
            .height
            .checked_add(1)
            .ok_or(DevnetError::InternalInvariant)?;
        let archived = persistence.finalized_block(height)?;
        if archived.previous != previous || archived.checkpoint.height != height {
            return Err(DevnetError::InternalInvariant);
        }
        let executed = executor
            .execute(previous, &state, &archived.payload)
            .map_err(|_| DevnetError::InternalInvariant)?;
        if executed.transition.commitment.block_hash != archived.checkpoint.block_hash
            || executed.transition.commitment.state_root != archived.checkpoint.state_root
        {
            return Err(DevnetError::InternalInvariant);
        }
        let operations = LedgerBlockCodec::decode(&archived.payload)
            .map_err(|_| DevnetError::InternalInvariant)?;
        let [signed] = operations.as_slice() else {
            return Err(DevnetError::InternalInvariant);
        };
        let AuthorizedOperation::Transfer(transfer) = signed.operation else {
            return Err(DevnetError::InternalInvariant);
        };
        let [receipt] = executed.receipts.as_slice() else {
            return Err(DevnetError::InternalInvariant);
        };
        if transfer.from == faucet {
            funded.insert(transfer.to);
        }
        let transaction = transaction_view(addresses, transfer, *receipt, archived.checkpoint)?;
        let checkout_id = CheckoutId::new(*transfer.idempotency_key.as_bytes());
        if let Some(checkout) = persistence.find_checkout(checkout_id)?
            && validate_transfer(&checkout, transfer).is_ok()
        {
            settled_checkouts.insert(checkout_id, transaction.clone());
        }
        let block = block_view(archived.checkpoint, 1);
        let prepared = index.prepare_append(previous, archived.checkpoint, block, transaction)?;
        index.commit(prepared)?;
        state = executed.transition.state;
        previous = archived.checkpoint;
    }
    if previous != latest.checkpoint || state != latest.state {
        return Err(DevnetError::InternalInvariant);
    }
    Ok((index, funded, settled_checkouts))
}

fn decode_signed(envelope: &str) -> Result<SignedOperation, DevnetError> {
    let bytes = decode_bounded_hex(envelope, MAX_ENVELOPE_BYTES)?;
    SignedOperationCodec::decode(&bytes).map_err(|_| DevnetError::InvalidEnvelope)
}

fn transaction_view(
    addresses: &AddressCodec,
    transfer: Transfer,
    receipt: ledger_core::OperationReceipt,
    checkpoint: FinalizedCheckpoint,
) -> Result<FinalizedTransactionView, DevnetError> {
    Ok(FinalizedTransactionView {
        id: encode_hex(receipt.operation_id.as_bytes()),
        block_height: checkpoint.height.to_string(),
        operation_index: receipt.operation_index.to_string(),
        from: addresses
            .encode(transfer.from)
            .map_err(|_| DevnetError::InternalInvariant)?,
        to: addresses
            .encode(transfer.to)
            .map_err(|_| DevnetError::InternalInvariant)?,
        amount: transfer.amount.to_string(),
        fee: transfer.fee.to_string(),
        outcome: match receipt.outcome {
            OperationOutcome::Applied => "applied",
            OperationOutcome::Expired => "expired",
        },
    })
}

fn block_view(checkpoint: FinalizedCheckpoint, transaction_count: usize) -> FinalizedBlockView {
    FinalizedBlockView {
        height: checkpoint.height.to_string(),
        hash: encode_hex(checkpoint.block_hash.as_bytes()),
        state_root: encode_root(checkpoint.state_root),
        transaction_count,
    }
}

fn encode_root(root: StateRoot) -> String {
    encode_hex(root.as_bytes())
}
