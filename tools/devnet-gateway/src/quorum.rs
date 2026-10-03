use std::{collections::BTreeMap, fs, path::Path, thread, time::Duration};

use consensus_signer_core::{Reservation, SigningJournal, VoteIntent, VoteSlot, VoteStage};
use consensus_signer_journal_fs::FileSigningJournal;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use finality_auth_ed25519::Ed25519FinalityVerifier;
use finality_proof_core::{
    ConsensusSignature, FinalityCertificate, FinalityCertificateCodec, FinalityVerifier, Validator,
    ValidatorSet, ValidatorSignature,
};
use network_membership_core::{ConsensusPublicKey, NodeId};
use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use state_sync_core::{FinalityProofVerifier, FinalizedCheckpoint, ValidatorSetHash};

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::{SyncBlockView, SyncCheckpointView},
    service::NETWORK,
    sync::{checkpoint_view, decode_checkpoint},
};

const MEMBERSHIP_EPOCH: u64 = 1;
const FINALITY_MODE: &str = "four-validator-quorum";
const VALIDATOR_COUNT: usize = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const STATUS_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Clone, Debug)]
pub(crate) enum FinalityPolicy {
    Single,
    Quorum(ValidatorSet),
}

impl FinalityPolicy {
    pub(crate) fn quorum(public_keys: &str) -> Result<Self, DevnetError> {
        parse_validator_set(public_keys).map(Self::Quorum)
    }

    pub(crate) fn validator_set_hash(&self) -> ValidatorSetHash {
        match self {
            Self::Single => ValidatorSetHash::new([57; 32]),
            Self::Quorum(set) => set.hash(),
        }
    }

    pub(crate) const fn mode(&self) -> &'static str {
        match self {
            Self::Single => "single-node-devnet",
            Self::Quorum(_) => FINALITY_MODE,
        }
    }

    pub(crate) fn validator_count(&self) -> usize {
        match self {
            Self::Single => 1,
            Self::Quorum(set) => set.validators().len(),
        }
    }

    pub(crate) const fn quorum_weight(&self) -> u64 {
        match self {
            Self::Single => 1,
            Self::Quorum(set) => set.quorum_weight(),
        }
    }
}

impl FinalityProofVerifier for FinalityPolicy {
    fn verify(
        &self,
        network: ledger_core::NetworkId,
        checkpoint: FinalizedCheckpoint,
        proof: &[u8],
    ) -> bool {
        match self {
            Self::Single => {
                network == NETWORK
                    && checkpoint.validator_set_hash == ValidatorSetHash::new([57; 32])
                    && proof == b"single-node-devnet-finality"
            }
            Self::Quorum(set) => FinalityVerifier::new(set.clone(), Ed25519FinalityVerifier)
                .verify(network, checkpoint, proof),
        }
    }
}

pub(crate) struct QuorumSetup {
    pub(crate) policy: FinalityPolicy,
    pub(crate) signer: QuorumSigner,
    pub(crate) coordinator_public_key: VerifyingKey,
    pub(crate) endpoints: Vec<String>,
}

impl QuorumSetup {
    pub(crate) fn from_environment(data_directory: &Path) -> Result<Option<Self>, DevnetError> {
        let Some(keys) = environment("DEVNET_VALIDATOR_PUBLIC_KEYS") else {
            return Ok(None);
        };
        let policy = FinalityPolicy::quorum(&keys)?;
        let FinalityPolicy::Quorum(validators) = &policy else {
            return Err(DevnetError::InvalidQuorumConfiguration);
        };
        let validators = validators.clone();
        let seed_path = environment("DEVNET_VALIDATOR_SEED_FILE")
            .ok_or(DevnetError::InvalidQuorumConfiguration)?;
        let seed = read_seed(Path::new(&seed_path))?;
        let signer = QuorumSigner::open(seed, validators.clone(), data_directory.join("signer"))?;
        let coordinator_hex = environment("DEVNET_COORDINATOR_PUBLIC_KEY")
            .ok_or(DevnetError::InvalidQuorumConfiguration)?;
        let coordinator_public_key = VerifyingKey::from_bytes(&decode_array(&coordinator_hex)?)
            .map_err(|_| DevnetError::InvalidQuorumConfiguration)?;
        let endpoints = environment("DEVNET_VALIDATOR_ENDPOINTS")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !endpoints.is_empty() && endpoints.len() != VALIDATOR_COUNT.saturating_sub(1) {
            return Err(DevnetError::InvalidQuorumConfiguration);
        }
        if !endpoints.is_empty() && signer.public_key() != coordinator_public_key {
            return Err(DevnetError::InvalidQuorumConfiguration);
        }
        Ok(Some(Self {
            policy,
            signer,
            coordinator_public_key,
            endpoints,
        }))
    }
}

pub(crate) struct QuorumSigner {
    node: NodeId,
    key: SigningKey,
    journal: FileSigningJournal,
    validator_set: ValidatorSet,
}

impl QuorumSigner {
    fn open(
        seed: [u8; 32],
        validator_set: ValidatorSet,
        journal_path: impl AsRef<Path>,
    ) -> Result<Self, DevnetError> {
        let key = SigningKey::from_bytes(&seed);
        let public = ConsensusPublicKey::new(key.verifying_key().to_bytes());
        let node = validator_set
            .validators()
            .iter()
            .find(|validator| validator.consensus_key == public)
            .map(|validator| validator.node)
            .ok_or(DevnetError::InvalidQuorumConfiguration)?;
        let journal =
            FileSigningJournal::open(journal_path).map_err(|_| DevnetError::StateUnavailable)?;
        Ok(Self {
            node,
            key,
            journal,
            validator_set,
        })
    }

    pub(crate) fn sign_checkpoint(
        &mut self,
        checkpoint: FinalizedCheckpoint,
    ) -> Result<ValidatorSignature, DevnetError> {
        let intent = VoteIntent {
            slot: VoteSlot {
                network: NETWORK,
                set_id: MEMBERSHIP_EPOCH,
                height: checkpoint.height,
                round: 0,
                stage: VoteStage::Precommit,
            },
            target_hash: checkpoint.block_hash,
        };
        match self
            .journal
            .reserve(intent)
            .map_err(|_| DevnetError::StateUnavailable)?
        {
            Reservation::New | Reservation::ExistingSame => {}
            Reservation::Conflict => return Err(DevnetError::ConflictingQuorumVote),
        }
        let certificate = FinalityCertificate {
            network: NETWORK,
            membership_epoch: MEMBERSHIP_EPOCH,
            round: 0,
            checkpoint,
            signatures: Vec::new(),
        };
        Ok(ValidatorSignature {
            node: self.node,
            signature: ConsensusSignature::new(
                self.key.sign(&certificate.signing_message()).to_bytes(),
            ),
        })
    }

    pub(crate) fn authorize(&self, proposal: &QuorumProposal) -> String {
        encode_hex(&self.key.sign(&proposal.authorization_message()).to_bytes())
    }

    pub(crate) const fn node(&self) -> NodeId {
        self.node
    }

    pub(crate) fn public_key(&self) -> VerifyingKey {
        self.key.verifying_key()
    }

    pub(crate) fn validator_set(&self) -> &ValidatorSet {
        &self.validator_set
    }
}

pub(crate) struct QuorumCoordinator {
    signer: QuorumSigner,
    endpoints: Vec<String>,
    client: Client,
}

impl QuorumCoordinator {
    pub(crate) fn new(signer: QuorumSigner, endpoints: Vec<String>) -> Result<Self, DevnetError> {
        let client = Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| DevnetError::InvalidQuorumConfiguration)?;
        Ok(Self {
            signer,
            endpoints,
            client,
        })
    }

    pub(crate) fn certify(
        &mut self,
        parent: FinalizedCheckpoint,
        checkpoint: FinalizedCheckpoint,
        payload: &[u8],
    ) -> Result<Vec<u8>, DevnetError> {
        let mut proposal = QuorumProposal {
            network_id: encode_hex(NETWORK.as_bytes()),
            parent: checkpoint_view(parent),
            checkpoint: checkpoint_view(checkpoint),
            payload: encode_hex(payload),
            authorization: String::new(),
        };
        proposal.authorization = self.signer.authorize(&proposal);
        let mut signatures = BTreeMap::new();
        let local = self.signer.sign_checkpoint(checkpoint)?;
        signatures.insert(local.node, local.signature);
        for endpoint in &self.endpoints {
            let url = internal_url(endpoint, "prepare")?;
            let response = self.client.post(url).json(&proposal).send();
            let Ok(response) = response else { continue };
            if response.status() != StatusCode::OK {
                continue;
            }
            let Ok(vote) = response.json::<QuorumVote>() else {
                continue;
            };
            let Ok(node) = decode_node(&vote.node) else {
                continue;
            };
            let Ok(signature) = decode_signature(&vote.signature) else {
                continue;
            };
            signatures.insert(node, signature);
            let certificate = certificate(checkpoint, &signatures);
            if FinalityVerifier::new(self.signer.validator_set().clone(), Ed25519FinalityVerifier)
                .verify_certificate(NETWORK, checkpoint, &certificate)
                .is_ok()
            {
                return FinalityCertificateCodec::encode(&certificate)
                    .map_err(|_| DevnetError::QuorumUnavailable);
            }
        }
        Err(DevnetError::QuorumUnavailable)
    }

    pub(crate) fn publish(&self, block: &SyncBlockView) {
        for endpoint in &self.endpoints {
            let Ok(url) = internal_url(endpoint, "commit") else {
                continue;
            };
            let _response = self.client.post(url).json(block).send();
        }
    }

    pub(crate) fn online_validators(&self, local_height: u64) -> usize {
        let remote_online = thread::scope(|scope| {
            self.endpoints
                .iter()
                .map(|endpoint| {
                    scope.spawn(move || {
                        let Ok(url) = internal_url(endpoint, "status") else {
                            return false;
                        };
                        let Ok(response) = self.client.get(url).timeout(STATUS_TIMEOUT).send()
                        else {
                            return false;
                        };
                        let Ok(status) = response.json::<QuorumValidatorStatus>() else {
                            return false;
                        };
                        status.finalized_height.parse::<u64>().ok() == Some(local_height)
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .filter(|online| *online)
                .count()
        });
        1_usize.saturating_add(remote_online)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuorumProposal {
    pub(crate) network_id: String,
    pub(crate) parent: SyncCheckpointView,
    pub(crate) checkpoint: SyncCheckpointView,
    pub(crate) payload: String,
    pub(crate) authorization: String,
}

impl QuorumProposal {
    fn authorization_message(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(b"payrail.quorum.proposal.v1\0");
        hasher.update(self.network_id.as_bytes());
        hasher.update(self.parent.height.as_bytes());
        hasher.update(self.parent.block_hash.as_bytes());
        hasher.update(self.parent.state_root.as_bytes());
        hasher.update(self.parent.validator_set_hash.as_bytes());
        hasher.update(self.checkpoint.height.as_bytes());
        hasher.update(self.checkpoint.block_hash.as_bytes());
        hasher.update(self.checkpoint.state_root.as_bytes());
        hasher.update(self.checkpoint.validator_set_hash.as_bytes());
        hasher.update(self.payload.as_bytes());
        hasher.finalize().to_vec()
    }

    pub(crate) fn verify_authorization(
        &self,
        coordinator: &VerifyingKey,
    ) -> Result<(), DevnetError> {
        let signature = ed25519_dalek::Signature::from_bytes(
            &decode_bounded_hex(&self.authorization, 64)?
                .try_into()
                .map_err(|_| DevnetError::InvalidQuorumRequest)?,
        );
        coordinator
            .verify_strict(&self.authorization_message(), &signature)
            .map_err(|_| DevnetError::InvalidQuorumRequest)
    }

    pub(crate) fn checkpoints(
        &self,
    ) -> Result<(FinalizedCheckpoint, FinalizedCheckpoint), DevnetError> {
        if self.network_id != encode_hex(NETWORK.as_bytes()) {
            return Err(DevnetError::InvalidQuorumRequest);
        }
        Ok((
            decode_checkpoint(&self.parent)?,
            decode_checkpoint(&self.checkpoint)?,
        ))
    }

    pub(crate) fn payload_bytes(&self) -> Result<Vec<u8>, DevnetError> {
        decode_bounded_hex(&self.payload, tail_sync_core::MAX_TAIL_PAYLOAD_BYTES)
            .map_err(|_| DevnetError::InvalidQuorumRequest)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QuorumVote {
    pub(crate) node: String,
    pub(crate) signature: String,
}

impl From<ValidatorSignature> for QuorumVote {
    fn from(value: ValidatorSignature) -> Self {
        Self {
            node: encode_hex(value.node.as_bytes()),
            signature: encode_hex(value.signature.as_bytes()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuorumValidatorStatus {
    pub(crate) node: String,
    pub(crate) finalized_height: String,
}

fn certificate(
    checkpoint: FinalizedCheckpoint,
    signatures: &BTreeMap<NodeId, ConsensusSignature>,
) -> FinalityCertificate {
    FinalityCertificate {
        network: NETWORK,
        membership_epoch: MEMBERSHIP_EPOCH,
        round: 0,
        checkpoint,
        signatures: signatures
            .iter()
            .map(|(node, signature)| ValidatorSignature {
                node: *node,
                signature: *signature,
            })
            .collect(),
    }
}

fn parse_validator_set(value: &str) -> Result<ValidatorSet, DevnetError> {
    let mut validators = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let public = decode_array(value)?;
            Ok(Validator {
                node: NodeId::new(public),
                consensus_key: ConsensusPublicKey::new(public),
                weight: 1,
            })
        })
        .collect::<Result<Vec<_>, DevnetError>>()?;
    if validators.len() != VALIDATOR_COUNT {
        return Err(DevnetError::InvalidQuorumConfiguration);
    }
    validators.sort_by_key(|validator| validator.node);
    ValidatorSet::new(NETWORK, MEMBERSHIP_EPOCH, validators)
        .map_err(|_| DevnetError::InvalidQuorumConfiguration)
}

fn read_seed(path: &Path) -> Result<[u8; 32], DevnetError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DevnetError::StateUnavailable)?;
    if !metadata.file_type().is_file() || metadata.len() != 32 {
        return Err(DevnetError::InvalidQuorumConfiguration);
    }
    fs::read(path)
        .map_err(|_| DevnetError::StateUnavailable)?
        .try_into()
        .map_err(|_| DevnetError::InvalidQuorumConfiguration)
}

pub(crate) fn public_key(path: &Path) -> Result<String, DevnetError> {
    Ok(encode_hex(
        SigningKey::from_bytes(&read_seed(path)?)
            .verifying_key()
            .as_bytes(),
    ))
}

pub(crate) fn account_address(path: &Path) -> Result<String, DevnetError> {
    let public_key = SigningKey::from_bytes(&read_seed(path)?)
        .verifying_key()
        .to_bytes();
    account_address::AddressCodec::new(NETWORK, crate::service::ADDRESS_PREFIX)
        .map_err(|_| DevnetError::InternalInvariant)?
        .encode(ledger_core::AccountId::new(public_key))
        .map_err(|_| DevnetError::InternalInvariant)
}

fn decode_array(value: &str) -> Result<[u8; 32], DevnetError> {
    decode_bounded_hex(value, 32)?
        .try_into()
        .map_err(|_| DevnetError::InvalidQuorumConfiguration)
}

fn decode_node(value: &str) -> Result<NodeId, DevnetError> {
    Ok(NodeId::new(decode_array(value)?))
}

fn decode_signature(value: &str) -> Result<ConsensusSignature, DevnetError> {
    let bytes = decode_bounded_hex(value, 64)?
        .try_into()
        .map_err(|_| DevnetError::InvalidQuorumRequest)?;
    Ok(ConsensusSignature::new(bytes))
}

fn internal_url(endpoint: &str, action: &str) -> Result<String, DevnetError> {
    if !endpoint.starts_with("http://") || endpoint.contains(['?', '#', '@']) {
        return Err(DevnetError::InvalidQuorumConfiguration);
    }
    Ok(format!(
        "{}/internal/quorum/{action}",
        endpoint.trim_end_matches('/')
    ))
}

fn environment(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use finality_proof_core::{FinalityVerifier, Validator};
    use state_sync_core::{BlockHash, StateRoot};

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("payrail-quorum-test-{}", std::process::id())))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _result = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn strict_quorum_certificate_and_vote_conflict_are_enforced() {
        let directory = TestDirectory::new();
        let keys = [
            SigningKey::from_bytes(&[1; 32]),
            SigningKey::from_bytes(&[2; 32]),
            SigningKey::from_bytes(&[3; 32]),
            SigningKey::from_bytes(&[4; 32]),
        ];
        let mut validators = keys
            .iter()
            .map(|key| {
                let public = key.verifying_key().to_bytes();
                Validator {
                    node: NodeId::new(public),
                    consensus_key: ConsensusPublicKey::new(public),
                    weight: 1,
                }
            })
            .collect::<Vec<_>>();
        validators.sort_by_key(|validator| validator.node);
        let set = ValidatorSet::new(NETWORK, MEMBERSHIP_EPOCH, validators).unwrap();
        let checkpoint = FinalizedCheckpoint {
            height: 9,
            block_hash: BlockHash::new([9; 32]),
            state_root: StateRoot::new([10; 32]),
            validator_set_hash: set.hash(),
        };
        let mut signers = keys
            .iter()
            .enumerate()
            .map(|(index, key)| {
                QuorumSigner::open(
                    key.to_bytes(),
                    set.clone(),
                    directory.0.join(index.to_string()),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let mut signatures = BTreeMap::new();
        for signer in signers.iter_mut().take(3) {
            let signature = signer.sign_checkpoint(checkpoint).unwrap();
            signatures.insert(signature.node, signature.signature);
        }
        let certificate = certificate(checkpoint, &signatures);
        assert!(
            FinalityVerifier::new(set.clone(), Ed25519FinalityVerifier)
                .verify_certificate(NETWORK, checkpoint, &certificate)
                .is_ok()
        );

        let conflicting = FinalizedCheckpoint {
            block_hash: BlockHash::new([11; 32]),
            ..checkpoint
        };
        assert_eq!(
            signers[0].sign_checkpoint(conflicting),
            Err(DevnetError::ConflictingQuorumVote)
        );
    }

    #[test]
    fn coordinator_authorization_rejects_mutated_payload() {
        let key = SigningKey::from_bytes(&[12; 32]);
        let checkpoint = SyncCheckpointView {
            height: "1".to_owned(),
            block_hash: encode_hex(&[1; 32]),
            state_root: encode_hex(&[2; 32]),
            validator_set_hash: encode_hex(&[3; 32]),
        };
        let mut proposal = QuorumProposal {
            network_id: encode_hex(NETWORK.as_bytes()),
            parent: checkpoint.clone(),
            checkpoint,
            payload: encode_hex(b"payload"),
            authorization: String::new(),
        };
        proposal.authorization =
            encode_hex(&key.sign(&proposal.authorization_message()).to_bytes());
        assert!(proposal.verify_authorization(&key.verifying_key()).is_ok());

        proposal.payload = encode_hex(b"different");
        assert_eq!(
            proposal.verify_authorization(&key.verifying_key()),
            Err(DevnetError::InvalidQuorumRequest)
        );
    }
}
