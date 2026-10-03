use ledger_core::NetworkId;
use network_membership_core::ProtocolDigest;
use sha2::{Digest, Sha256};

use crate::StateSyncError;

macro_rules! byte_identifier {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 32]);

        impl $name {
            #[must_use]
            pub const fn new(value: [u8; 32]) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

byte_identifier!(BlockHash);
byte_identifier!(StateRoot);
byte_identifier!(ValidatorSetHash);
byte_identifier!(SnapshotFormatId);
byte_identifier!(ChunkHash);
byte_identifier!(ManifestId);

impl ChunkHash {
    #[must_use]
    pub fn digest(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"state-sync.chunk\0");
        hasher.update(bytes);
        Self(hasher.finalize().into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedCheckpoint {
    pub height: u64,
    pub block_hash: BlockHash,
    pub state_root: StateRoot,
    pub validator_set_hash: ValidatorSetHash,
}

pub trait FinalityProofVerifier {
    fn verify(&self, network: NetworkId, checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkDescriptor {
    pub index: u32,
    pub offset: u64,
    pub length: u32,
    pub hash: ChunkHash,
}

/// Persistence boundary for chunks that passed manifest and content checks.
///
/// Implementations must make a successful write durable and idempotent before
/// returning. The sync session never advances in-memory progress first.
pub trait VerifiedChunkStore {
    type Error;

    /// Durably and idempotently stores one already verified chunk.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error when durable publication
    /// fails. Callers must not advance sync progress after an error.
    fn persist_verified(
        &mut self,
        manifest: ManifestId,
        descriptor: ChunkDescriptor,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
}

/// Read boundary used to reconstruct sync progress after a process restart.
/// Implementations must distinguish an absent chunk from corruption or I/O
/// failure and return only bytes that still match the descriptor.
pub trait VerifiedChunkReader {
    type Error;

    /// Loads one previously persisted chunk, or `None` when it is absent.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error for corruption or read failure.
    fn load_verified(
        &self,
        manifest: ManifestId,
        descriptor: ChunkDescriptor,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotManifest {
    pub network: NetworkId,
    pub protocol: ProtocolDigest,
    pub snapshot_format: SnapshotFormatId,
    pub checkpoint: FinalizedCheckpoint,
    pub total_bytes: u64,
    pub chunks: Vec<ChunkDescriptor>,
}

impl SnapshotManifest {
    /// Computes a content identifier over the canonical manifest fields.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest layout or bounds are invalid.
    pub fn manifest_id(&self) -> Result<ManifestId, StateSyncError> {
        self.validate()?;
        let mut hasher = Sha256::new();
        hasher.update(b"state-sync.manifest\0");
        hasher.update(self.network.as_bytes());
        hasher.update(self.protocol.as_bytes());
        hasher.update(self.snapshot_format.as_bytes());
        hasher.update(self.checkpoint.height.to_be_bytes());
        hasher.update(self.checkpoint.block_hash.as_bytes());
        hasher.update(self.checkpoint.state_root.as_bytes());
        hasher.update(self.checkpoint.validator_set_hash.as_bytes());
        hasher.update(self.total_bytes.to_be_bytes());
        let chunk_count =
            u32::try_from(self.chunks.len()).map_err(|_| StateSyncError::TooManyChunks)?;
        hasher.update(chunk_count.to_be_bytes());
        for chunk in &self.chunks {
            hasher.update(chunk.index.to_be_bytes());
            hasher.update(chunk.offset.to_be_bytes());
            hasher.update(chunk.length.to_be_bytes());
            hasher.update(chunk.hash.as_bytes());
        }
        Ok(ManifestId(hasher.finalize().into()))
    }

    pub(crate) fn validate(&self) -> Result<(), StateSyncError> {
        if self.chunks.is_empty() {
            return Err(StateSyncError::EmptyManifest);
        }
        if self.chunks.len() > crate::MAX_SNAPSHOT_CHUNKS {
            return Err(StateSyncError::TooManyChunks);
        }
        if self.total_bytes > crate::MAX_SNAPSHOT_BYTES {
            return Err(StateSyncError::SnapshotTooLarge);
        }
        let mut expected_offset = 0_u64;
        for (expected_index, chunk) in self.chunks.iter().enumerate() {
            let expected_index =
                u32::try_from(expected_index).map_err(|_| StateSyncError::TooManyChunks)?;
            if chunk.index != expected_index || chunk.offset != expected_offset || chunk.length == 0
            {
                return Err(StateSyncError::NonCanonicalChunkLayout);
            }
            let length =
                usize::try_from(chunk.length).map_err(|_| StateSyncError::ChunkTooLarge)?;
            if length > crate::MAX_CHUNK_BYTES {
                return Err(StateSyncError::ChunkTooLarge);
            }
            expected_offset = expected_offset
                .checked_add(u64::from(chunk.length))
                .ok_or(StateSyncError::SnapshotTooLarge)?;
        }
        if expected_offset != self.total_bytes {
            return Err(StateSyncError::TotalSizeMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkAcceptance {
    Added,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncProgress {
    pub received_chunks: usize,
    pub total_chunks: usize,
    pub received_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncCompletion {
    pub manifest: ManifestId,
    pub checkpoint: FinalizedCheckpoint,
}
