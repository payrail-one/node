use external_asset_core::{
    FinalityEvidence, OriginDeposit, OriginReleaseReference, SettlementReference, WithdrawalId,
};
use external_asset_settlement_core::SignedWithdrawalRequest;
use ledger_core::{Authorization, NetworkId};

const AUTHORIZATION_DOMAIN: &[u8] = b"external.settlement.operation.authorization.v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeAuthorizationRole {
    Observer,
    Finality,
    Settlement,
    Release,
}

impl BridgeAuthorizationRole {
    const fn discriminant(self) -> u8 {
        match self {
            Self::Observer => 0,
            Self::Finality => 1,
            Self::Settlement => 2,
            Self::Release => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepositSettlementOperation {
    pub network: NetworkId,
    pub claim: OriginDeposit,
    pub evidence: FinalityEvidence,
    pub reference: SettlementReference,
    pub observer_authorization: Authorization,
    pub finality_authorization: Authorization,
    pub settlement_authorization: Authorization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WithdrawalSettlementOperation {
    pub request: SignedWithdrawalRequest,
    pub settlement_authorization: Authorization,
    pub release_authorization: Authorization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteWithdrawalOperation {
    pub network: NetworkId,
    pub withdrawal: WithdrawalId,
    pub release_reference: OriginReleaseReference,
    pub evidence: FinalityEvidence,
    pub release_authorization: Authorization,
    pub finality_authorization: Authorization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalSettlementOperation {
    Deposit(DepositSettlementOperation),
    Withdrawal(WithdrawalSettlementOperation),
    CompleteWithdrawal(CompleteWithdrawalOperation),
}

impl DepositSettlementOperation {
    #[must_use]
    pub fn authorization_message(&self, role: BridgeAuthorizationRole) -> Vec<u8> {
        let mut output = authorization_prefix(role, 0, self.network);
        encode_origin_deposit(&mut output, self.claim);
        encode_finality(&mut output, self.evidence);
        output.extend_from_slice(self.reference.as_bytes());
        output
    }
}

impl WithdrawalSettlementOperation {
    #[must_use]
    pub fn authorization_message(&self, role: BridgeAuthorizationRole) -> Vec<u8> {
        let mut output = authorization_prefix(role, 1, self.request.network);
        output.extend_from_slice(&self.request.authorization_message());
        output.extend_from_slice(self.request.signature.as_bytes());
        output
    }
}

impl CompleteWithdrawalOperation {
    #[must_use]
    pub fn authorization_message(&self, role: BridgeAuthorizationRole) -> Vec<u8> {
        let mut output = authorization_prefix(role, 2, self.network);
        output.extend_from_slice(self.withdrawal.as_bytes());
        output.extend_from_slice(self.release_reference.transaction.as_bytes());
        output.extend_from_slice(&self.release_reference.event_index.to_be_bytes());
        encode_finality(&mut output, self.evidence);
        output
    }
}

fn authorization_prefix(
    role: BridgeAuthorizationRole,
    operation: u8,
    network: NetworkId,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(256);
    output.extend_from_slice(AUTHORIZATION_DOMAIN);
    output.push(role.discriminant());
    output.push(operation);
    output.extend_from_slice(network.as_bytes());
    output
}

pub(crate) fn encode_origin_deposit(output: &mut Vec<u8>, claim: OriginDeposit) {
    output.extend_from_slice(claim.asset.as_bytes());
    output.extend_from_slice(claim.origin_network.as_bytes());
    output.extend_from_slice(claim.origin_asset.as_bytes());
    output.extend_from_slice(claim.transaction.as_bytes());
    output.extend_from_slice(&claim.event_index.to_be_bytes());
    output.extend_from_slice(claim.recipient.as_bytes());
    output.extend_from_slice(&claim.amount.to_be_bytes());
}

pub(crate) fn encode_finality(output: &mut Vec<u8>, evidence: FinalityEvidence) {
    output.extend_from_slice(&evidence.block_height.to_be_bytes());
    output.extend_from_slice(&evidence.block_hash);
    output.extend_from_slice(&evidence.proof_hash);
}
