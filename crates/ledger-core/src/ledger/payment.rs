use std::collections::BTreeMap;

use crate::{
    AccountId, AccountStatus, AssetId, AuthorizedOperation, Balance, Event, IdempotencyKey,
    LedgerError, MAX_BATCH_ITEMS, Nonce, OperationId, OperationKind, OperationOutcome,
    OperationReceipt, Transfer, TransferBatch, TransferItem,
};

use super::{Ledger, checked_add, has_backing_deficit};

impl Ledger {
    /// Applies one signed-origin transfer with an optional same-asset fee.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid authorization, amount, nonce or asset
    /// status; restricted accounts; insufficient funds; or arithmetic overflow.
    pub fn transfer(
        &mut self,
        origin: AccountId,
        transfer: Transfer,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_transfer(origin, transfer)?;
        let operation_id = AuthorizedOperation::Transfer(transfer).operation_id()?;
        self.apply_transfer(
            transfer,
            transfer.from,
            OperationKind::Transfer,
            operation_id,
        )
    }

    /// Applies a transfer where a separately authorized account pays the fee.
    ///
    /// The future signed transaction envelope must bind both authorizations to
    /// the complete payment payload. This reference API models those verified
    /// origins explicitly and never infers sponsor consent.
    ///
    /// # Errors
    ///
    /// Returns an error when either origin is unauthorized, sponsorship is not
    /// applicable, or any ordinary transfer invariant fails.
    pub fn transfer_sponsored(
        &mut self,
        sender_origin: AccountId,
        fee_payer_origin: AccountId,
        fee_payer: AccountId,
        transfer: Transfer,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_transfer(sender_origin, transfer)?;
        self.validate_fee_sponsor(
            fee_payer_origin,
            fee_payer,
            transfer.asset,
            transfer.from,
            transfer.fee,
        )?;
        let operation_id = AuthorizedOperation::SponsoredTransfer {
            transfer,
            fee_payer,
        }
        .operation_id()?;
        self.apply_transfer(
            transfer,
            fee_payer,
            OperationKind::SponsoredTransfer,
            operation_id,
        )
    }

    fn apply_transfer(
        &mut self,
        transfer: Transfer,
        fee_payer: AccountId,
        kind: OperationKind,
        operation_id: OperationId,
    ) -> Result<OperationReceipt, LedgerError> {
        let treasury = self.asset(transfer.asset)?.definition.treasury;
        let updates = self.calculate_transfer_balances(transfer, fee_payer, treasury)?;
        let next_nonce = self
            .nonce(transfer.from)
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let receipt = self.create_receipt(
            operation_id,
            transfer.from,
            transfer.idempotency_key,
            transfer.nonce,
            kind,
            OperationOutcome::Applied,
        );
        let next_operation_index = receipt
            .operation_index
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;

        for (account, balance) in updates {
            self.set_balance(transfer.asset, account, balance);
        }
        self.nonces.insert(transfer.from, next_nonce);
        self.next_operation_index = next_operation_index;
        self.events.push(Event::Transferred {
            operation_id,
            idempotency_key: transfer.idempotency_key,
            asset: transfer.asset,
            from: transfer.from,
            to: transfer.to,
            fee_payer,
            amount: transfer.amount,
            fee: transfer.fee,
            nonce: transfer.nonce,
        });
        Ok(receipt)
    }

    /// Applies multiple same-asset payments as one atomic operation.
    ///
    /// A batch consumes one sender nonce and charges one aggregate fee. No
    /// balance, nonce, operation sequence or event is changed unless every
    /// payment is valid.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid authorization, nonce, asset status, batch
    /// size or payment amount; restricted accounts; insufficient funds; or
    /// arithmetic overflow.
    pub fn transfer_batch(
        &mut self,
        origin: AccountId,
        batch: TransferBatch,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_batch(origin, &batch)?;
        let operation_id = AuthorizedOperation::TransferBatch(batch.clone()).operation_id()?;
        self.apply_batch(batch, origin, OperationKind::BatchTransfer, operation_id)
    }

    /// Applies an atomic batch where a separately authorized account pays the
    /// aggregate fee.
    ///
    /// # Errors
    ///
    /// Returns an error when either origin is unauthorized, sponsorship is not
    /// applicable, or any ordinary batch invariant fails.
    pub fn transfer_batch_sponsored(
        &mut self,
        sender_origin: AccountId,
        fee_payer_origin: AccountId,
        fee_payer: AccountId,
        batch: TransferBatch,
    ) -> Result<OperationReceipt, LedgerError> {
        self.validate_batch(sender_origin, &batch)?;
        self.validate_fee_sponsor(
            fee_payer_origin,
            fee_payer,
            batch.asset,
            batch.from,
            batch.fee,
        )?;
        let operation_id = AuthorizedOperation::SponsoredBatchTransfer {
            batch: batch.clone(),
            fee_payer,
        }
        .operation_id()?;
        self.apply_batch(
            batch,
            fee_payer,
            OperationKind::SponsoredBatchTransfer,
            operation_id,
        )
    }

    fn apply_batch(
        &mut self,
        batch: TransferBatch,
        fee_payer: AccountId,
        kind: OperationKind,
        operation_id: OperationId,
    ) -> Result<OperationReceipt, LedgerError> {
        let treasury = self.asset(batch.asset)?.definition.treasury;
        let updates = self.calculate_batch_balances(&batch, fee_payer, treasury)?;
        let next_nonce = self
            .nonce(batch.from)
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let receipt = self.create_receipt(
            operation_id,
            batch.from,
            batch.idempotency_key,
            batch.nonce,
            kind,
            OperationOutcome::Applied,
        );
        let next_operation_index = receipt
            .operation_index
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;

        for (account, balance) in updates {
            self.set_balance(batch.asset, account, balance);
        }
        self.nonces.insert(batch.from, next_nonce);
        self.next_operation_index = next_operation_index;
        self.events.push(Event::BatchTransferred {
            operation_id,
            idempotency_key: batch.idempotency_key,
            asset: batch.asset,
            from: batch.from,
            fee_payer,
            items: batch.items,
            fee: batch.fee,
            nonce: batch.nonce,
        });
        Ok(receipt)
    }

    fn validate_transfer(&self, origin: AccountId, transfer: Transfer) -> Result<(), LedgerError> {
        if transfer.network != self.network {
            return Err(LedgerError::WrongNetwork);
        }
        if origin != transfer.from {
            return Err(LedgerError::Unauthorized);
        }
        if transfer.amount == 0 {
            return Err(LedgerError::InvalidAmount);
        }
        if transfer.from == transfer.to {
            return Err(LedgerError::SameAccount);
        }

        self.validate_asset_operation(transfer.asset)?;
        validate_sender_status(self.account_status(transfer.asset, transfer.from))?;
        validate_recipient_status(self.account_status(transfer.asset, transfer.to))?;
        self.validate_fee_recipient(transfer.asset, transfer.from, transfer.fee)?;
        self.validate_nonce(transfer.from, transfer.nonce)
    }

    fn validate_batch(&self, origin: AccountId, batch: &TransferBatch) -> Result<(), LedgerError> {
        if batch.network != self.network {
            return Err(LedgerError::WrongNetwork);
        }
        if origin != batch.from {
            return Err(LedgerError::Unauthorized);
        }
        if batch.items.is_empty() {
            return Err(LedgerError::EmptyBatch);
        }
        if batch.items.len() > MAX_BATCH_ITEMS {
            return Err(LedgerError::BatchTooLarge);
        }

        self.validate_asset_operation(batch.asset)?;
        validate_sender_status(self.account_status(batch.asset, batch.from))?;
        for item in &batch.items {
            self.validate_batch_item(batch.asset, batch.from, *item)?;
        }
        self.validate_fee_recipient(batch.asset, batch.from, batch.fee)?;
        self.validate_nonce(batch.from, batch.nonce)
    }

    fn validate_asset_operation(&self, asset: AssetId) -> Result<(), LedgerError> {
        let state = self.asset(asset)?;
        if !state.definition.status.allows_transfer() {
            return Err(LedgerError::AssetNotTransferable);
        }
        if has_backing_deficit(state) {
            return Err(LedgerError::BackingDeficit);
        }
        Ok(())
    }

    fn validate_batch_item(
        &self,
        asset: AssetId,
        from: AccountId,
        item: TransferItem,
    ) -> Result<(), LedgerError> {
        if item.amount == 0 {
            return Err(LedgerError::InvalidAmount);
        }
        if item.to == from {
            return Err(LedgerError::SameAccount);
        }
        validate_recipient_status(self.account_status(asset, item.to))
    }

    fn validate_fee_recipient(
        &self,
        asset: AssetId,
        from: AccountId,
        fee: Balance,
    ) -> Result<(), LedgerError> {
        if fee == 0 {
            return Ok(());
        }
        let treasury = self.asset(asset)?.definition.treasury;
        if treasury == from {
            return Ok(());
        }
        validate_recipient_status(self.account_status(asset, treasury))
    }

    fn validate_fee_sponsor(
        &self,
        fee_payer_origin: AccountId,
        fee_payer: AccountId,
        asset: AssetId,
        sender: AccountId,
        fee: Balance,
    ) -> Result<(), LedgerError> {
        if fee_payer_origin != fee_payer {
            return Err(LedgerError::Unauthorized);
        }
        if fee == 0 || fee_payer == sender {
            return Err(LedgerError::FeeSponsorshipNotApplicable);
        }
        validate_sender_status(self.account_status(asset, fee_payer))
    }

    pub(super) fn validate_nonce(
        &self,
        account: AccountId,
        nonce: Nonce,
    ) -> Result<(), LedgerError> {
        let expected = self.nonce(account);
        if nonce != expected {
            return Err(LedgerError::NonceMismatch {
                expected,
                actual: nonce,
            });
        }
        Ok(())
    }

    pub(super) fn create_receipt(
        &self,
        operation_id: OperationId,
        account: AccountId,
        idempotency_key: IdempotencyKey,
        nonce: Nonce,
        kind: OperationKind,
        outcome: OperationOutcome,
    ) -> OperationReceipt {
        OperationReceipt {
            operation_id,
            account,
            idempotency_key,
            nonce,
            operation_index: self.next_operation_index,
            kind,
            outcome,
        }
    }

    fn calculate_transfer_balances(
        &self,
        transfer: Transfer,
        fee_payer: AccountId,
        treasury: AccountId,
    ) -> Result<BTreeMap<AccountId, Balance>, LedgerError> {
        let mut updates = BTreeMap::from([
            (transfer.from, self.balance(transfer.asset, transfer.from)),
            (transfer.to, self.balance(transfer.asset, transfer.to)),
            (fee_payer, self.balance(transfer.asset, fee_payer)),
            (treasury, self.balance(transfer.asset, treasury)),
        ]);

        if fee_payer == transfer.from {
            debit_balance(
                &mut updates,
                transfer.from,
                checked_add(transfer.amount, transfer.fee)?,
            )?;
        } else {
            debit_balance(&mut updates, transfer.from, transfer.amount)?;
            debit_balance(&mut updates, fee_payer, transfer.fee)?;
        }
        credit_balance(&mut updates, transfer.to, transfer.amount)?;
        credit_balance(&mut updates, treasury, transfer.fee)?;
        Ok(updates)
    }

    fn calculate_batch_balances(
        &self,
        batch: &TransferBatch,
        fee_payer: AccountId,
        treasury: AccountId,
    ) -> Result<BTreeMap<AccountId, Balance>, LedgerError> {
        let total_amount = batch.items.iter().try_fold(0_u128, |total, item| {
            total
                .checked_add(item.amount)
                .ok_or(LedgerError::ArithmeticOverflow)
        })?;
        let mut updates = BTreeMap::from([
            (batch.from, self.balance(batch.asset, batch.from)),
            (fee_payer, self.balance(batch.asset, fee_payer)),
            (treasury, self.balance(batch.asset, treasury)),
        ]);

        if fee_payer == batch.from {
            debit_balance(
                &mut updates,
                batch.from,
                checked_add(total_amount, batch.fee)?,
            )?;
        } else {
            debit_balance(&mut updates, batch.from, total_amount)?;
            debit_balance(&mut updates, fee_payer, batch.fee)?;
        }
        for item in &batch.items {
            let current = updates
                .get(&item.to)
                .copied()
                .unwrap_or_else(|| self.balance(batch.asset, item.to));
            updates.insert(item.to, checked_add(current, item.amount)?);
        }
        credit_balance(&mut updates, treasury, batch.fee)?;
        Ok(updates)
    }
}

fn validate_sender_status(status: AccountStatus) -> Result<(), LedgerError> {
    if status.allows_send() {
        return Ok(());
    }
    if status == AccountStatus::Frozen {
        return Err(LedgerError::AccountFrozen);
    }
    Err(LedgerError::AccountCannotSend)
}

fn validate_recipient_status(status: AccountStatus) -> Result<(), LedgerError> {
    if status.allows_receive() {
        return Ok(());
    }
    if status == AccountStatus::Frozen {
        return Err(LedgerError::AccountFrozen);
    }
    Err(LedgerError::AccountCannotReceive)
}

fn debit_balance(
    balances: &mut BTreeMap<AccountId, Balance>,
    account: AccountId,
    amount: Balance,
) -> Result<(), LedgerError> {
    let current = balances.get(&account).copied().unwrap_or(0);
    let updated = current
        .checked_sub(amount)
        .ok_or(LedgerError::InsufficientBalance)?;
    balances.insert(account, updated);
    Ok(())
}

fn credit_balance(
    balances: &mut BTreeMap<AccountId, Balance>,
    account: AccountId,
    amount: Balance,
) -> Result<(), LedgerError> {
    let current = balances.get(&account).copied().unwrap_or(0);
    balances.insert(account, checked_add(current, amount)?);
    Ok(())
}
