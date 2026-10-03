use std::collections::BTreeSet;

use ledger_core::NetworkId;
use network_membership_core::{Admission, PeerRole, ProtocolDigest};

use crate::{
    ChunkAcceptance, ChunkHash, FinalityProofVerifier, ManifestId, SnapshotManifest, StateRoot,
    StateSyncError, SyncCompletion, SyncProgress, VerifiedChunkReader, VerifiedChunkStore,
};

pub const MAX_CHUNK_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT_CHUNKS: usize = 1_000_000;
pub const MAX_SNAPSHOT_BYTES: u64 = 16 * 1024 * 1024 * 1024 * 1024;
pub const MAX_FINALITY_PROOF_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub struct StateSyncSession {
    expected_membership_epoch: u64,
    manifest: SnapshotManifest,
    manifest_id: ManifestId,
    received: BTreeSet<u32>,
    received_bytes: u64,
}

impl StateSyncSession {
    /// Starts a bounded sync session only after checkpoint finality verification.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed manifests, wrong network/protocol,
    /// oversized proof or failed finality verification.
    pub fn start<V: FinalityProofVerifier>(
        expected_network: NetworkId,
        expected_protocol: ProtocolDigest,
        expected_membership_epoch: u64,
        manifest: SnapshotManifest,
        finality_proof: &[u8],
        verifier: &V,
    ) -> Result<Self, StateSyncError> {
        if manifest.network != expected_network {
            return Err(StateSyncError::WrongNetwork);
        }
        if manifest.protocol != expected_protocol {
            return Err(StateSyncError::UnsupportedProtocol);
        }
        if finality_proof.len() > MAX_FINALITY_PROOF_BYTES {
            return Err(StateSyncError::FinalityProofTooLarge);
        }
        let manifest_id = manifest.manifest_id()?;
        if !verifier.verify(manifest.network, manifest.checkpoint, finality_proof) {
            return Err(StateSyncError::InvalidFinalityProof);
        }
        Ok(Self {
            expected_membership_epoch,
            manifest,
            manifest_id,
            received: BTreeSet::new(),
            received_bytes: 0,
        })
    }

    /// Verifies an out-of-order chunk from an admitted sync provider.
    ///
    /// The supplied store durably persists verified bytes before in-memory
    /// progress advances. A crash between those steps is safe: retrying the
    /// same content-addressed write must be idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error for an unauthorized provider, unknown index, wrong
    /// length/hash or progress overflow.
    pub fn accept_chunk<S: VerifiedChunkStore>(
        &mut self,
        provider: Admission,
        index: u32,
        bytes: &[u8],
        store: &mut S,
    ) -> Result<ChunkAcceptance, StateSyncError> {
        self.validate_provider(provider)?;
        let descriptor = self
            .manifest
            .chunks
            .get(usize::try_from(index).map_err(|_| StateSyncError::UnknownChunk)?)
            .filter(|descriptor| descriptor.index == index)
            .ok_or(StateSyncError::UnknownChunk)?;
        if bytes.len()
            != usize::try_from(descriptor.length)
                .map_err(|_| StateSyncError::ChunkLengthMismatch)?
        {
            return Err(StateSyncError::ChunkLengthMismatch);
        }
        if ChunkHash::digest(bytes) != descriptor.hash {
            return Err(StateSyncError::ChunkHashMismatch);
        }
        if self.received.contains(&index) {
            return Ok(ChunkAcceptance::Duplicate);
        }
        store
            .persist_verified(self.manifest_id, *descriptor, bytes)
            .map_err(|_| StateSyncError::StorageFailure)?;
        self.received_bytes = self
            .received_bytes
            .checked_add(u64::from(descriptor.length))
            .ok_or(StateSyncError::ProgressOverflow)?;
        self.received.insert(index);
        Ok(ChunkAcceptance::Added)
    }

    #[must_use]
    pub fn progress(&self) -> SyncProgress {
        SyncProgress {
            received_chunks: self.received.len(),
            total_chunks: self.manifest.chunks.len(),
            received_bytes: self.received_bytes,
            total_bytes: self.manifest.total_bytes,
        }
    }

    #[must_use]
    pub fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn manifest_id(&self) -> ManifestId {
        self.manifest_id
    }

    /// Reconstructs in-memory progress from durably stored verified chunks.
    /// Missing chunks remain pending; corrupt storage fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error for storage failure, descriptor mismatch or progress
    /// overflow. Existing in-memory entries remain idempotent.
    pub fn recover_persisted<R: VerifiedChunkReader>(
        &mut self,
        reader: &R,
    ) -> Result<SyncProgress, StateSyncError> {
        for descriptor in &self.manifest.chunks {
            let Some(bytes) = reader
                .load_verified(self.manifest_id, *descriptor)
                .map_err(|_| StateSyncError::StorageFailure)?
            else {
                continue;
            };
            if bytes.len()
                != usize::try_from(descriptor.length)
                    .map_err(|_| StateSyncError::StoredChunkCorrupt)?
                || ChunkHash::digest(&bytes) != descriptor.hash
            {
                return Err(StateSyncError::StoredChunkCorrupt);
            }
            if !self.received.contains(&descriptor.index) {
                let recovered_bytes = self
                    .received_bytes
                    .checked_add(u64::from(descriptor.length))
                    .ok_or(StateSyncError::ProgressOverflow)?;
                self.received.insert(descriptor.index);
                self.received_bytes = recovered_bytes;
            }
        }
        Ok(self.progress())
    }

    /// Completes only after every chunk and the applied state root are verified.
    ///
    /// # Errors
    ///
    /// Returns an error when chunks are missing or state application produced a
    /// root different from the finalized checkpoint.
    pub fn finish(&self, applied_state_root: StateRoot) -> Result<SyncCompletion, StateSyncError> {
        if self.received.len() != self.manifest.chunks.len()
            || self.received_bytes != self.manifest.total_bytes
        {
            return Err(StateSyncError::SyncIncomplete);
        }
        if applied_state_root != self.manifest.checkpoint.state_root {
            return Err(StateSyncError::StateRootMismatch);
        }
        Ok(SyncCompletion {
            manifest: self.manifest_id,
            checkpoint: self.manifest.checkpoint,
        })
    }

    fn validate_provider(&self, provider: Admission) -> Result<(), StateSyncError> {
        if provider.network != self.manifest.network {
            return Err(StateSyncError::ProviderWrongNetwork);
        }
        if provider.membership_epoch != self.expected_membership_epoch {
            return Err(StateSyncError::ProviderEpochMismatch);
        }
        if provider.role != PeerRole::SyncProvider {
            return Err(StateSyncError::ProviderRoleNotAllowed);
        }
        Ok(())
    }
}
