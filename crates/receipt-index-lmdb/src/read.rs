use heed::{Database, types::Bytes};
use ledger_core::{AccountId, IdempotencyKey, Nonce, OperationId};

use crate::{
    IndexedReceipt, ReceiptIndexStoreError, codec::decode_receipt, store::LmdbReceiptIndex,
};

impl LmdbReceiptIndex {
    /// Finds a finalized receipt by its content-derived operation identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the stored record is corrupt or LMDB fails.
    pub fn by_operation_id(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        self.receipt_for_id(&transaction, operation_id)
    }

    /// Finds a finalized receipt by its global operation index.
    ///
    /// # Errors
    ///
    /// Returns an error when a secondary index is dangling or corrupt.
    pub fn by_operation_index(
        &self,
        operation_index: u64,
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        let receipt = self.receipt_from_database(
            &transaction,
            self.operation_indices,
            &operation_index.to_be_bytes(),
        )?;
        if receipt.is_some_and(|value| value.receipt.operation_index != operation_index) {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(receipt)
    }

    /// Finds a finalized receipt by sender and consumed nonce.
    ///
    /// # Errors
    ///
    /// Returns an error when a secondary index is dangling or corrupt.
    pub fn by_account_nonce(
        &self,
        account: AccountId,
        nonce: Nonce,
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        let key = account_nonce_key(account, nonce);
        let receipt = self.receipt_from_database(&transaction, self.account_nonces, &key)?;
        if receipt
            .is_some_and(|value| value.receipt.account != account || value.receipt.nonce != nonce)
        {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(receipt)
    }

    /// Finds one finalized receipt by signed client correlation data and nonce.
    ///
    /// The nonce is part of the key because the ledger deliberately permits a
    /// client idempotency key to be reused in a later consensus operation.
    ///
    /// # Errors
    ///
    /// Returns an error when a secondary index is dangling or corrupt.
    pub fn by_correlation(
        &self,
        account: AccountId,
        idempotency_key: IdempotencyKey,
        nonce: Nonce,
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        let key = correlation_key(account, idempotency_key, nonce);
        let receipt = self.receipt_from_database(&transaction, self.correlations, &key)?;
        if receipt.is_some_and(|value| {
            value.receipt.account != account
                || value.receipt.idempotency_key != idempotency_key
                || value.receipt.nonce != nonce
        }) {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(receipt)
    }

    pub(crate) fn receipt_from_database(
        &self,
        transaction: &heed::RoTxn<'_>,
        database: Database<Bytes, Bytes>,
        key: &[u8],
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let Some(operation_id) = database.get(transaction, key)? else {
            return Ok(None);
        };
        let operation_id = OperationId::new(
            operation_id
                .try_into()
                .map_err(|_| ReceiptIndexStoreError::CorruptRecord)?,
        );
        self.receipt_for_id(transaction, operation_id)
    }

    pub(crate) fn receipt_for_id(
        &self,
        transaction: &heed::RoTxn<'_>,
        operation_id: OperationId,
    ) -> Result<Option<IndexedReceipt>, ReceiptIndexStoreError> {
        let Some(encoded) = self
            .receipts
            .get(transaction, operation_id.as_bytes().as_slice())?
        else {
            return Ok(None);
        };
        let receipt = decode_receipt(encoded)?;
        if receipt.receipt.operation_id != operation_id {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(Some(receipt))
    }
}

pub(crate) fn account_nonce_key(account: AccountId, nonce: Nonce) -> [u8; 40] {
    let mut key = [0_u8; 40];
    key[..32].copy_from_slice(account.as_bytes());
    key[32..].copy_from_slice(&nonce.to_be_bytes());
    key
}

pub(crate) fn correlation_key(
    account: AccountId,
    idempotency_key: IdempotencyKey,
    nonce: Nonce,
) -> [u8; 72] {
    let mut key = [0_u8; 72];
    key[..32].copy_from_slice(account.as_bytes());
    key[32..64].copy_from_slice(idempotency_key.as_bytes());
    key[64..].copy_from_slice(&nonce.to_be_bytes());
    key
}
