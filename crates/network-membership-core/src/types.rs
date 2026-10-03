use ledger_core::NetworkId;

macro_rules! byte_identifier {
    ($name:ident, $size:expr) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; $size]);

        impl $name {
            #[must_use]
            pub const fn new(value: [u8; $size]) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $size] {
                &self.0
            }
        }
    };
}

byte_identifier!(NodeId, 32);
byte_identifier!(TransportPublicKey, 32);
byte_identifier!(ConsensusPublicKey, 32);
byte_identifier!(CertificateFingerprint, 32);
byte_identifier!(Challenge, 32);
byte_identifier!(ProtocolDigest, 32);
byte_identifier!(TransportProof, 64);

impl Default for TransportProof {
    fn default() -> Self {
        Self([0; 64])
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NodeCapabilities {
    pub validator: bool,
    pub sync_provider: bool,
    pub sentry: bool,
}

impl NodeCapabilities {
    pub(crate) const fn is_empty(self) -> bool {
        !self.validator && !self.sync_provider && !self.sentry
    }

    pub(crate) const fn permits(self, role: PeerRole) -> bool {
        match role {
            PeerRole::Validator => self.validator,
            PeerRole::SyncProvider => self.sync_provider,
            PeerRole::Sentry => self.sentry,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeRecord {
    pub node: NodeId,
    pub transport_key: TransportPublicKey,
    pub certificate_fingerprint: CertificateFingerprint,
    pub consensus_key: Option<ConsensusPublicKey>,
    pub capabilities: NodeCapabilities,
    pub active_from: u64,
    pub revoked_from: Option<u64>,
    pub metadata_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerRole {
    Validator,
    SyncProvider,
    Sentry,
}

impl PeerRole {
    const fn discriminant(self) -> u8 {
        match self {
            Self::Validator => 0,
            Self::SyncProvider => 1,
            Self::Sentry => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerHandshake {
    pub network: NetworkId,
    pub node: NodeId,
    pub membership_epoch: u64,
    pub role: PeerRole,
    pub protocol: ProtocolDigest,
    pub challenge: Challenge,
    pub proof: TransportProof,
}

impl PeerHandshake {
    #[must_use]
    pub fn authorization_bytes(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(155);
        message.extend_from_slice(b"network.peer-auth\0");
        message.extend_from_slice(self.network.as_bytes());
        message.extend_from_slice(self.node.as_bytes());
        message.extend_from_slice(&self.membership_epoch.to_be_bytes());
        message.push(self.role.discriminant());
        message.extend_from_slice(self.protocol.as_bytes());
        message.extend_from_slice(self.challenge.as_bytes());
        message
    }
}

pub trait TransportProofVerifier {
    fn verify(&self, public_key: TransportPublicKey, message: &[u8], proof: TransportProof)
    -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admission {
    pub network: NetworkId,
    pub node: NodeId,
    pub role: PeerRole,
    pub membership_epoch: u64,
    pub protocol: ProtocolDigest,
    pub challenge: Challenge,
    pub transport_key: TransportPublicKey,
    pub certificate_fingerprint: CertificateFingerprint,
    pub consensus_key: Option<ConsensusPublicKey>,
}
