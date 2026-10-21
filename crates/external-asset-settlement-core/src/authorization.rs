use external_asset_core::WithdrawalRequest;
use ledger_core::{NetworkId, SignatureBytes, SignatureVerifier};

use crate::ExternalSettlementError;

const WITHDRAWAL_AUTHORIZATION_DOMAIN: &[u8] = b"external.withdrawal.authorization.v1\0";

/// A user withdrawal request carrying the account owner's authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedWithdrawalRequest {
    pub network: NetworkId,
    pub request: WithdrawalRequest,
    pub valid_until_height: u64,
    pub signature: SignatureBytes,
}

impl SignedWithdrawalRequest {
    /// Produces the canonical bytes signed by the withdrawing account.
    #[must_use]
    pub fn authorization_message(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(WITHDRAWAL_AUTHORIZATION_DOMAIN.len() + 184);
        output.extend_from_slice(WITHDRAWAL_AUTHORIZATION_DOMAIN);
        output.extend_from_slice(self.network.as_bytes());
        output.extend_from_slice(self.request.asset.as_bytes());
        output.extend_from_slice(self.request.account.as_bytes());
        output.extend_from_slice(&self.request.amount.to_be_bytes());
        output.extend_from_slice(self.request.destination.as_bytes());
        output.extend_from_slice(self.request.burn_reference.as_bytes());
        output.extend_from_slice(&self.valid_until_height.to_be_bytes());
        output
    }

    /// Verifies the network-bound owner signature and creates a process-local
    /// capability. The current height is checked again when the burn executes.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, expired request or invalid owner
    /// signature.
    pub fn verify<V: SignatureVerifier>(
        self,
        expected_network: NetworkId,
        current_height: u64,
        verifier: &V,
    ) -> Result<AuthorizedWithdrawal, ExternalSettlementError> {
        if self.network != expected_network {
            return Err(ExternalSettlementError::WrongWithdrawalNetwork);
        }
        if current_height > self.valid_until_height {
            return Err(ExternalSettlementError::ExpiredWithdrawalAuthorization);
        }
        if !verifier.verify(
            self.request.account,
            &self.authorization_message(),
            self.signature,
        ) {
            return Err(ExternalSettlementError::InvalidWithdrawalSignature);
        }
        Ok(AuthorizedWithdrawal {
            network: self.network,
            request: self.request,
            valid_until_height: self.valid_until_height,
        })
    }
}

/// A process-local capability proving that the withdrawing account signed the
/// complete network, asset, amount, destination, replay reference and expiry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizedWithdrawal {
    pub(crate) network: NetworkId,
    pub(crate) request: WithdrawalRequest,
    pub(crate) valid_until_height: u64,
}

impl AuthorizedWithdrawal {
    #[must_use]
    pub const fn request(self) -> WithdrawalRequest {
        self.request
    }
}
