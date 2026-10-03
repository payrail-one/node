use std::collections::BTreeMap;

use ledger_core::{AccountId, NetworkId};

use crate::{
    Admission, CertificateFingerprint, Challenge, ConsensusPublicKey, MembershipError, NodeId,
    NodeRecord, PeerHandshake, ProtocolDigest, TransportProofVerifier, TransportPublicKey,
};

#[derive(Debug)]
pub struct MembershipRegistry {
    network: NetworkId,
    governance_authority: AccountId,
    epoch: u64,
    nodes: BTreeMap<NodeId, NodeRecord>,
    transport_keys: BTreeMap<TransportPublicKey, NodeId>,
    certificate_fingerprints: BTreeMap<CertificateFingerprint, NodeId>,
    consensus_keys: BTreeMap<ConsensusPublicKey, NodeId>,
}

impl MembershipRegistry {
    #[must_use]
    pub fn new(network: NetworkId, governance_authority: AccountId) -> Self {
        Self {
            network,
            governance_authority,
            epoch: 0,
            nodes: BTreeMap::new(),
            transport_keys: BTreeMap::new(),
            certificate_fingerprints: BTreeMap::new(),
            consensus_keys: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Registers a node and advances the membership epoch.
    ///
    /// # Errors
    ///
    /// Returns an error for unauthorized governance, invalid role/key shape,
    /// duplicate identities or epoch overflow.
    pub fn register_node(
        &mut self,
        origin: AccountId,
        record: NodeRecord,
    ) -> Result<u64, MembershipError> {
        self.require_governance(origin)?;
        validate_record(record)?;
        if self.nodes.contains_key(&record.node) {
            return Err(MembershipError::NodeAlreadyRegistered);
        }
        if self.transport_keys.contains_key(&record.transport_key) {
            return Err(MembershipError::TransportKeyAlreadyRegistered);
        }
        if self
            .certificate_fingerprints
            .contains_key(&record.certificate_fingerprint)
        {
            return Err(MembershipError::CertificateAlreadyRegistered);
        }
        if record
            .consensus_key
            .is_some_and(|key| self.consensus_keys.contains_key(&key))
        {
            return Err(MembershipError::ConsensusKeyAlreadyRegistered);
        }
        let next_epoch = self.next_epoch()?;
        self.transport_keys
            .insert(record.transport_key, record.node);
        self.certificate_fingerprints
            .insert(record.certificate_fingerprint, record.node);
        if let Some(key) = record.consensus_key {
            self.consensus_keys.insert(key, record.node);
        }
        self.nodes.insert(record.node, record);
        self.epoch = next_epoch;
        Ok(next_epoch)
    }

    /// Schedules revocation at a block height and advances the epoch.
    ///
    /// # Errors
    ///
    /// Returns an error for unauthorized governance, an unknown/already revoked
    /// node, a height before activation or epoch overflow.
    pub fn revoke_node(
        &mut self,
        origin: AccountId,
        node: NodeId,
        revoked_from: u64,
    ) -> Result<u64, MembershipError> {
        self.require_governance(origin)?;
        let record = self
            .nodes
            .get(&node)
            .copied()
            .ok_or(MembershipError::NodeNotFound)?;
        if record.revoked_from.is_some() {
            return Err(MembershipError::AlreadyRevoked);
        }
        if revoked_from < record.active_from {
            return Err(MembershipError::InvalidRevocationHeight);
        }
        let next_epoch = self.next_epoch()?;
        self.nodes
            .get_mut(&node)
            .ok_or(MembershipError::NodeNotFound)?
            .revoked_from = Some(revoked_from);
        self.epoch = next_epoch;
        Ok(next_epoch)
    }

    /// Authenticates one fresh peer handshake against network, epoch, height,
    /// registered role and transport-key possession.
    ///
    /// # Errors
    ///
    /// Returns an error for any mismatch or invalid proof.
    pub fn admit<V: TransportProofVerifier>(
        &self,
        current_height: u64,
        expected_protocol: ProtocolDigest,
        expected_challenge: Challenge,
        handshake: &PeerHandshake,
        verifier: &V,
    ) -> Result<Admission, MembershipError> {
        if handshake.network != self.network {
            return Err(MembershipError::WrongNetwork);
        }
        if handshake.membership_epoch != self.epoch {
            return Err(MembershipError::StaleMembershipEpoch);
        }
        if handshake.protocol != expected_protocol {
            return Err(MembershipError::UnsupportedProtocol);
        }
        if handshake.challenge != expected_challenge {
            return Err(MembershipError::ChallengeMismatch);
        }
        let record = self
            .nodes
            .get(&handshake.node)
            .copied()
            .ok_or(MembershipError::NodeNotFound)?;
        if current_height < record.active_from {
            return Err(MembershipError::NotYetActive);
        }
        if record
            .revoked_from
            .is_some_and(|height| current_height >= height)
        {
            return Err(MembershipError::Revoked);
        }
        if !record.capabilities.permits(handshake.role) {
            return Err(MembershipError::RoleNotAllowed);
        }
        if !verifier.verify(
            record.transport_key,
            &handshake.authorization_bytes(),
            handshake.proof,
        ) {
            return Err(MembershipError::InvalidProof);
        }
        Ok(Admission {
            network: self.network,
            node: record.node,
            role: handshake.role,
            membership_epoch: self.epoch,
            protocol: handshake.protocol,
            challenge: handshake.challenge,
            transport_key: record.transport_key,
            certificate_fingerprint: record.certificate_fingerprint,
            consensus_key: record.consensus_key,
        })
    }

    #[must_use]
    pub fn node(&self, node: NodeId) -> Option<NodeRecord> {
        self.nodes.get(&node).copied()
    }

    fn require_governance(&self, origin: AccountId) -> Result<(), MembershipError> {
        if origin == self.governance_authority {
            Ok(())
        } else {
            Err(MembershipError::Unauthorized)
        }
    }

    fn next_epoch(&self) -> Result<u64, MembershipError> {
        self.epoch
            .checked_add(1)
            .ok_or(MembershipError::EpochOverflow)
    }
}

fn validate_record(record: NodeRecord) -> Result<(), MembershipError> {
    if record.capabilities.is_empty() {
        return Err(MembershipError::InvalidCapabilities);
    }
    if record.capabilities.validator != record.consensus_key.is_some() {
        return Err(MembershipError::InvalidConsensusKey);
    }
    if record
        .revoked_from
        .is_some_and(|height| height < record.active_from)
    {
        return Err(MembershipError::InvalidRevocationHeight);
    }
    Ok(())
}
