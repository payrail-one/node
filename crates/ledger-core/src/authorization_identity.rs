use sha2::{Digest, Sha256};

use crate::LedgerError;
use crate::authorization::{Authorization, SignedOperation};

const VERIFIED_AUTHORIZATION_DOMAIN: &[u8] = b"ledger.verified-authorization.v1\0";

/// Stable process-cache identity for one complete signed operation.
///
/// Unlike an operation ID, this identity includes every authorization byte. It
/// is suitable only as a local cache key and is not a consensus or wire ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VerifiedAuthorizationId([u8; 32]);

impl VerifiedAuthorizationId {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedAuthorizationIdentity {
    id: VerifiedAuthorizationId,
    encoded_bytes: usize,
}

impl VerifiedAuthorizationIdentity {
    #[must_use]
    pub const fn id(self) -> VerifiedAuthorizationId {
        self.id
    }

    #[must_use]
    pub const fn encoded_bytes(self) -> usize {
        self.encoded_bytes
    }
}

impl SignedOperation {
    /// Derives a domain-separated identity for the full signed envelope.
    ///
    /// The operation's canonical bytes already bind the network and every
    /// monetary field. Both role-bound signer identities and signature bytes
    /// are added here so an operation ID alone can never authorize a cache hit.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation cannot be canonically encoded.
    pub fn verified_authorization_identity(
        &self,
    ) -> Result<VerifiedAuthorizationIdentity, LedgerError> {
        let operation = self.operation.canonical_bytes()?;
        let fee_payer_bytes = if self.fee_payer_authorization.is_some() {
            1_usize + 32 + 64
        } else {
            1
        };
        let encoded_bytes = operation
            .len()
            .checked_add(32 + 64)
            .and_then(|bytes| bytes.checked_add(fee_payer_bytes))
            .ok_or(LedgerError::BatchTooLarge)?;
        let mut hasher = Sha256::new();
        hasher.update(VERIFIED_AUTHORIZATION_DOMAIN);
        hasher.update(
            u64::try_from(operation.len())
                .map_err(|_| LedgerError::BatchTooLarge)?
                .to_be_bytes(),
        );
        hasher.update(operation);
        hash_authorization(&mut hasher, self.sender_authorization);
        match self.fee_payer_authorization {
            Some(authorization) => {
                hasher.update([1]);
                hash_authorization(&mut hasher, authorization);
            }
            None => hasher.update([0]),
        }
        Ok(VerifiedAuthorizationIdentity {
            id: VerifiedAuthorizationId(hasher.finalize().into()),
            encoded_bytes,
        })
    }

    /// Returns the full signed-envelope cache key without exposing a shortcut
    /// for creating a [`crate::VerifiedOperation`].
    ///
    /// # Errors
    ///
    /// Returns an error when the operation cannot be canonically encoded.
    pub fn verified_authorization_id(&self) -> Result<VerifiedAuthorizationId, LedgerError> {
        self.verified_authorization_identity()
            .map(VerifiedAuthorizationIdentity::id)
    }
}

fn hash_authorization(hasher: &mut Sha256, authorization: Authorization) {
    hasher.update(authorization.signer.as_bytes());
    hasher.update(authorization.signature.as_bytes());
}
