use crate::{AccountId, AccountStatus, AssetId, Event, LedgerError};

use super::Ledger;

impl Ledger {
    /// Changes one account's transfer permissions for one asset.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset does not exist or the caller is not its
    /// configured freeze authority.
    pub fn set_account_status(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        account: AccountId,
        status: AccountStatus,
    ) -> Result<(), LedgerError> {
        if origin != self.asset(asset)?.definition.freeze_authority {
            return Err(LedgerError::Unauthorized);
        }

        if status == AccountStatus::Active {
            self.account_statuses.remove(&(asset, account));
        } else {
            self.account_statuses.insert((asset, account), status);
        }
        self.events.push(Event::AccountStatusChanged {
            asset,
            account,
            status,
        });
        Ok(())
    }

    /// Compatibility helper for callers that only need full freeze/unfreeze.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::set_account_status`].
    pub fn set_account_frozen(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        account: AccountId,
        frozen: bool,
    ) -> Result<(), LedgerError> {
        let status = if frozen {
            AccountStatus::Frozen
        } else {
            AccountStatus::Active
        };
        self.set_account_status(origin, asset, account, status)
    }

    #[must_use]
    pub fn account_status(&self, asset: AssetId, account: AccountId) -> AccountStatus {
        self.account_statuses
            .get(&(asset, account))
            .copied()
            .unwrap_or_default()
    }
}
