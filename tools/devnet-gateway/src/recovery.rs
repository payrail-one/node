use std::collections::{BTreeMap, BTreeSet};

use account_address::AddressCodec;
use external_asset_runtime_core::{
    ExternalSettlementBlockExecutor, ExternalSettlementExecutionKind,
};
use ledger_core::{AccountId, AuthorizedOperation};
use merchant_checkout_core::CheckoutId;
use transaction_auth_ed25519::Ed25519Verifier;

use crate::{
    DevnetError, bridge::bridge_transaction_view, checkout::validate_transfer,
    contract_view::transaction_view, index::ExplorerIndex, model::FinalizedTransactionView,
    persistence::DevnetPersistence, service::block_view,
};

pub(crate) type RebuiltPresentation = (
    ExplorerIndex,
    BTreeSet<AccountId>,
    BTreeMap<CheckoutId, FinalizedTransactionView>,
);

pub(crate) fn rebuild_presentation(
    persistence: &DevnetPersistence,
    executor: &ExternalSettlementBlockExecutor<Ed25519Verifier>,
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
            .map_err(|error| {
                eprintln!("devnet recovery execution failed at height {height}: {error:?}");
                DevnetError::InternalInvariant
            })?;
        if executed.transition.commitment.block_hash != archived.checkpoint.block_hash
            || executed.transition.commitment.state_root != archived.checkpoint.state_root
        {
            eprintln!("devnet recovery commitment mismatch at height {height}");
            return Err(DevnetError::InternalInvariant);
        }
        let settlement = executor
            .settlement_state(&executed.transition.state)
            .map_err(|_| DevnetError::InternalInvariant)?;
        let transaction = match &executed.kind {
            ExternalSettlementExecutionKind::Ledger(operations) => {
                let [signed] = operations.as_slice() else {
                    return Err(DevnetError::InternalInvariant);
                };
                let [receipt] = executed.receipts.as_slice() else {
                    return Err(DevnetError::InternalInvariant);
                };
                if let AuthorizedOperation::Transfer(transfer) = signed.operation
                    && transfer.from == faucet
                {
                    funded.insert(transfer.to);
                }
                let transaction =
                    transaction_view(addresses, &signed.operation, *receipt, archived.checkpoint)?;
                if let AuthorizedOperation::Transfer(transfer) = signed.operation {
                    let checkout_id = CheckoutId::new(*transfer.idempotency_key.as_bytes());
                    if let Some(checkout) = persistence.find_checkout(checkout_id)?
                        && validate_transfer(&checkout, transfer).is_ok()
                    {
                        settled_checkouts.insert(checkout_id, transaction.clone());
                    }
                }
                transaction
            }
            kind => bridge_transaction_view(addresses, archived.checkpoint, kind, &settlement)?,
        };
        let block = block_view(archived.checkpoint, 1);
        let prepared = index.prepare_append(previous, archived.checkpoint, block, transaction)?;
        index.commit(prepared)?;
        state = executed.transition.state;
        previous = archived.checkpoint;
    }
    if previous != latest.checkpoint || state != latest.state {
        eprintln!(
            "devnet recovery final state mismatch at height {}",
            previous.height
        );
        return Err(DevnetError::InternalInvariant);
    }
    Ok((index, funded, settled_checkouts))
}
