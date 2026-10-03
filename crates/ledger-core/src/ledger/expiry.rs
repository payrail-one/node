use crate::{AuthorizedOperation, Event, LedgerError, OperationOutcome, OperationReceipt};

use super::Ledger;

impl Ledger {
    pub(crate) fn expire_authorized(
        &mut self,
        operation: &AuthorizedOperation,
        block_height: u64,
    ) -> Result<OperationReceipt, LedgerError> {
        if operation.network() != self.network {
            return Err(LedgerError::WrongNetwork);
        }
        let account = operation.sender();
        let nonce = operation.nonce();
        self.validate_nonce(account, nonce)?;
        let next_nonce = nonce
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        let operation_id = operation.operation_id()?;
        let receipt = self.create_receipt(
            operation_id,
            account,
            operation.idempotency_key(),
            nonce,
            operation.kind(),
            OperationOutcome::Expired,
        );
        let next_operation_index = receipt
            .operation_index
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;

        self.nonces.insert(account, next_nonce);
        self.next_operation_index = next_operation_index;
        self.events.push(Event::OperationExpired {
            operation_id,
            idempotency_key: operation.idempotency_key(),
            account,
            nonce,
            kind: operation.kind(),
            valid_until_height: operation.valid_until_height(),
            finalized_at_height: block_height,
        });
        Ok(receipt)
    }
}
