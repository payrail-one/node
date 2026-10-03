use ledger_core::{LedgerSnapshot, NetworkId};
use state_sync_core::StateRoot;

use crate::{LedgerStateCodec, RuntimeError, authenticated_state_root, state_root};

/// Consensus-visible algorithm used to commit one canonical ledger state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateCommitmentScheme {
    /// Domain-separated SHA-256 over the complete canonical state image.
    CanonicalStateV1,
    /// Versioned Jellyfish Merkle Tree over normalized ledger rows.
    AuthenticatedStateV1,
}

/// Immutable network policy for a one-way state-commitment migration.
///
/// The activation height is part of network configuration. Every checkpoint
/// below it uses the canonical-image hash; the checkpoint at the activation
/// height and every checkpoint after it use the authenticated-state root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateCommitmentPolicy {
    authenticated_from_height: Option<u64>,
}

impl StateCommitmentPolicy {
    /// Keeps the canonical-image commitment for every height.
    #[must_use]
    pub const fn canonical_state_v1() -> Self {
        Self {
            authenticated_from_height: None,
        }
    }

    /// Activates authenticated normalized-state commitments at `height`.
    #[must_use]
    pub const fn authenticated_state_v1_from(height: u64) -> Self {
        Self {
            authenticated_from_height: Some(height),
        }
    }

    #[must_use]
    pub const fn authenticated_from_height(self) -> Option<u64> {
        self.authenticated_from_height
    }

    #[must_use]
    pub const fn scheme_at(self, height: u64) -> StateCommitmentScheme {
        match self.authenticated_from_height {
            Some(activation) if height >= activation => StateCommitmentScheme::AuthenticatedStateV1,
            Some(_) | None => StateCommitmentScheme::CanonicalStateV1,
        }
    }

    /// Computes the consensus root for a bounded canonical state image.
    ///
    /// # Errors
    ///
    /// Returns an error when authenticated commitment is active and the state
    /// image cannot be decoded into a valid ledger snapshot.
    pub fn root_for_state(
        self,
        height: u64,
        network: NetworkId,
        canonical_state: &[u8],
    ) -> Result<StateRoot, RuntimeError> {
        match self.scheme_at(height) {
            StateCommitmentScheme::CanonicalStateV1 => Ok(state_root(canonical_state)),
            StateCommitmentScheme::AuthenticatedStateV1 => {
                let snapshot = LedgerStateCodec::decode(network, canonical_state)?;
                authenticated_state_root(&snapshot)
            }
        }
    }

    pub(crate) fn root_for_snapshot(
        self,
        height: u64,
        canonical_state: &[u8],
        snapshot: &LedgerSnapshot,
    ) -> Result<StateRoot, RuntimeError> {
        match self.scheme_at(height) {
            StateCommitmentScheme::CanonicalStateV1 => Ok(state_root(canonical_state)),
            StateCommitmentScheme::AuthenticatedStateV1 => authenticated_state_root(snapshot),
        }
    }
}

impl Default for StateCommitmentPolicy {
    fn default() -> Self {
        Self::canonical_state_v1()
    }
}
