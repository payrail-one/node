use crate::{
    AccountId, ContractCall, ContractDeploy, IdempotencyKey, Ledger, LedgerError, MAX_BATCH_ITEMS,
    MAX_CONTRACT_ARGS_BYTES, MAX_CONTRACT_CODE_BYTES, NetworkId, Nonce, OperationId,
    OperationReceipt, Transfer, TransferBatch, TransferItem,
};
use sha2::{Digest, Sha256};

const AUTHORIZATION_DOMAIN: &[u8] = b"ledger.authorization\0";
const OPERATION_ID_DOMAIN: &[u8] = b"ledger.operation-id\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignatureBytes([u8; 64]);

impl SignatureBytes {
    #[must_use]
    pub const fn new(value: [u8; 64]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SignatureVerification<'a> {
    signer: AccountId,
    message: &'a [u8],
    signature: SignatureBytes,
}

impl<'a> SignatureVerification<'a> {
    #[must_use]
    pub const fn new(signer: AccountId, message: &'a [u8], signature: SignatureBytes) -> Self {
        Self {
            signer,
            message,
            signature,
        }
    }

    #[must_use]
    pub const fn signer(self) -> AccountId {
        self.signer
    }

    #[must_use]
    pub const fn message(self) -> &'a [u8] {
        self.message
    }

    #[must_use]
    pub const fn signature(self) -> SignatureBytes {
        self.signature
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationRole {
    Sender,
    FeePayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Authorization {
    pub signer: AccountId,
    pub signature: SignatureBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthorizedOperation {
    Transfer(Transfer),
    TransferBatch(TransferBatch),
    SponsoredTransfer {
        transfer: Transfer,
        fee_payer: AccountId,
    },
    SponsoredBatchTransfer {
        batch: TransferBatch,
        fee_payer: AccountId,
    },
    ContractDeploy(ContractDeploy),
    ContractCall(ContractCall),
}

impl AuthorizedOperation {
    #[must_use]
    pub const fn sender(&self) -> AccountId {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => transfer.from,
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => batch.from,
            Self::ContractDeploy(deploy) => deploy.owner,
            Self::ContractCall(call) => call.caller,
        }
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => transfer.network,
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => {
                batch.network
            }
            Self::ContractDeploy(deploy) => deploy.network,
            Self::ContractCall(call) => call.network,
        }
    }

    #[must_use]
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => {
                transfer.idempotency_key
            }
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => {
                batch.idempotency_key
            }
            Self::ContractDeploy(deploy) => deploy.idempotency_key,
            Self::ContractCall(call) => call.idempotency_key,
        }
    }

    #[must_use]
    pub const fn nonce(&self) -> Nonce {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => transfer.nonce,
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => batch.nonce,
            Self::ContractDeploy(deploy) => deploy.nonce,
            Self::ContractCall(call) => call.nonce,
        }
    }

    #[must_use]
    pub const fn valid_until_height(&self) -> u64 {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => {
                transfer.valid_until_height
            }
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => {
                batch.valid_until_height
            }
            Self::ContractDeploy(deploy) => deploy.valid_until_height,
            Self::ContractCall(call) => call.valid_until_height,
        }
    }

    #[must_use]
    pub const fn fee(&self) -> crate::Balance {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => transfer.fee,
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => batch.fee,
            Self::ContractDeploy(deploy) => deploy.fee,
            Self::ContractCall(call) => call.fee,
        }
    }

    #[must_use]
    pub const fn asset(&self) -> crate::AssetId {
        match self {
            Self::Transfer(transfer) | Self::SponsoredTransfer { transfer, .. } => transfer.asset,
            Self::TransferBatch(batch) | Self::SponsoredBatchTransfer { batch, .. } => batch.asset,
            Self::ContractDeploy(deploy) => deploy.asset,
            Self::ContractCall(call) => call.asset,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> crate::OperationKind {
        match self {
            Self::Transfer(_) => crate::OperationKind::Transfer,
            Self::TransferBatch(_) => crate::OperationKind::BatchTransfer,
            Self::SponsoredTransfer { .. } => crate::OperationKind::SponsoredTransfer,
            Self::SponsoredBatchTransfer { .. } => crate::OperationKind::SponsoredBatchTransfer,
            Self::ContractDeploy(_) => crate::OperationKind::ContractDeploy,
            Self::ContractCall(_) => crate::OperationKind::ContractCall,
        }
    }

    #[must_use]
    pub const fn fee_payer(&self) -> Option<AccountId> {
        match self {
            Self::Transfer(_)
            | Self::TransferBatch(_)
            | Self::ContractDeploy(_)
            | Self::ContractCall(_) => None,
            Self::SponsoredTransfer { fee_payer, .. }
            | Self::SponsoredBatchTransfer { fee_payer, .. } => Some(*fee_payer),
        }
    }

    /// Produces the canonical bytes covered by one authorization role.
    ///
    /// The encoding uses fixed-width big-endian integers, explicit operation
    /// and role discriminants, and includes the network domain and every field
    /// that can change monetary effects.
    ///
    /// # Errors
    ///
    /// Returns an error when a batch exceeds the protocol bound.
    pub fn authorization_message(&self, role: AuthorizationRole) -> Result<Vec<u8>, LedgerError> {
        let mut output = Vec::new();
        output.extend_from_slice(AUTHORIZATION_DOMAIN);
        output.push(role_discriminant(role));
        output.extend_from_slice(&self.canonical_bytes()?);
        Ok(output)
    }

    /// Encodes the complete monetary operation in one canonical representation.
    ///
    /// # Errors
    ///
    /// Returns an error when a batch exceeds the protocol bound.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, LedgerError> {
        let mut output = Vec::new();
        match self {
            Self::Transfer(transfer) => {
                output.push(0);
                encode_transfer(&mut output, *transfer);
            }
            Self::TransferBatch(batch) => {
                output.push(1);
                encode_batch(&mut output, batch)?;
            }
            Self::SponsoredTransfer {
                transfer,
                fee_payer,
            } => {
                output.push(2);
                encode_transfer(&mut output, *transfer);
                output.extend_from_slice(fee_payer.as_bytes());
            }
            Self::SponsoredBatchTransfer { batch, fee_payer } => {
                output.push(3);
                encode_batch(&mut output, batch)?;
                output.extend_from_slice(fee_payer.as_bytes());
            }
            Self::ContractDeploy(deploy) => {
                output.push(4);
                encode_contract_deploy(&mut output, deploy)?;
            }
            Self::ContractCall(call) => {
                output.push(5);
                encode_contract_call(&mut output, call)?;
            }
        }
        Ok(output)
    }

    /// Computes a stable content identifier independent of signatures.
    ///
    /// # Errors
    ///
    /// Returns an error when a batch exceeds the protocol bound.
    pub fn operation_id(&self) -> Result<OperationId, LedgerError> {
        let mut hasher = Sha256::new();
        hasher.update(OPERATION_ID_DOMAIN);
        hasher.update(self.canonical_bytes()?);
        Ok(OperationId::new(hasher.finalize().into()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedOperation {
    pub operation: AuthorizedOperation,
    pub sender_authorization: Authorization,
    pub fee_payer_authorization: Option<Authorization>,
}

impl SignedOperation {
    /// Verifies the complete role-bound authorization set without executing state.
    ///
    /// This is the shared signature boundary for API and gossip admission. Ledger
    /// execution calls the same method before applying monetary invariants.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed signer set, invalid canonical operation
    /// or failed signature verification.
    pub fn verify_authorizations<V: SignatureVerifier>(
        &self,
        verifier: &V,
    ) -> Result<(), LedgerError> {
        verify_authorizations(verifier, self)
    }

    /// Converts this operation into a process-local authorization capability.
    ///
    /// The capability is deliberately not serializable and can only be created
    /// through the shared network and role-bound verification path. Runtime
    /// implementations may verify a bounded batch in parallel and then apply
    /// the resulting capabilities in deterministic consensus order.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, malformed signer set, invalid
    /// canonical operation or failed signature verification.
    pub fn verify_for_network<V: SignatureVerifier>(
        self,
        network: NetworkId,
        verifier: &V,
    ) -> Result<VerifiedOperation, LedgerError> {
        if self.operation.network() != network {
            return Err(LedgerError::WrongNetwork);
        }
        self.verify_authorizations(verifier)?;
        Ok(VerifiedOperation {
            network,
            signed: self,
        })
    }
}

/// A process-local operation whose complete authorization set has been checked.
///
/// Construction is private so callers cannot accidentally bypass the
/// authoritative authorization rules. The verifier implementation remains a
/// trusted cryptographic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedOperation {
    network: NetworkId,
    signed: SignedOperation,
}

impl VerifiedOperation {
    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn operation(&self) -> &AuthorizedOperation {
        &self.signed.operation
    }

    #[must_use]
    pub const fn signed(&self) -> &SignedOperation {
        &self.signed
    }

    #[must_use]
    pub fn into_signed(self) -> SignedOperation {
        self.signed
    }
}

pub trait SignatureVerifier {
    fn verify(&self, signer: AccountId, message: &[u8], signature: SignatureBytes) -> bool;

    fn verify_batch(&self, verifications: &[SignatureVerification<'_>]) -> bool {
        verifications.iter().all(|verification| {
            self.verify(
                verification.signer(),
                verification.message(),
                verification.signature(),
            )
        })
    }
}

/// Verifies a bounded operation group while preserving input and error order.
///
/// A verifier may use one cryptographic batch equation for the valid path. If
/// that equation fails, every operation is checked individually so callers
/// still receive the same deterministic error positions as sequential
/// verification.
pub fn verify_operation_batch<V: SignatureVerifier>(
    network: NetworkId,
    verifier: &V,
    operations: Vec<SignedOperation>,
) -> Vec<Result<VerifiedOperation, LedgerError>> {
    let prepared = operations
        .into_iter()
        .map(|signed| prepare_operation(network, signed))
        .collect::<Vec<_>>();
    let verifications = prepared
        .iter()
        .filter_map(|operation| operation.as_ref().ok())
        .flat_map(PreparedOperation::verifications)
        .collect::<Vec<_>>();

    if verifier.verify_batch(&verifications) {
        return prepared.into_iter().map(finalize_prepared).collect();
    }

    prepared
        .into_iter()
        .map(|operation| {
            let operation = operation?;
            if verifier.verify_batch(&operation.verifications().collect::<Vec<_>>()) {
                Ok(operation.into_verified())
            } else {
                Err(LedgerError::InvalidSignature)
            }
        })
        .collect()
}

struct PreparedOperation {
    network: NetworkId,
    signed: SignedOperation,
    messages: Vec<(Authorization, Vec<u8>)>,
}

impl PreparedOperation {
    fn verifications(&self) -> impl Iterator<Item = SignatureVerification<'_>> {
        self.messages.iter().map(|(authorization, message)| {
            SignatureVerification::new(authorization.signer, message, authorization.signature)
        })
    }

    fn into_verified(self) -> VerifiedOperation {
        VerifiedOperation {
            network: self.network,
            signed: self.signed,
        }
    }
}

fn prepare_operation(
    network: NetworkId,
    signed: SignedOperation,
) -> Result<PreparedOperation, LedgerError> {
    if signed.operation.network() != network {
        return Err(LedgerError::WrongNetwork);
    }
    if signed.sender_authorization.signer != signed.operation.sender() {
        return Err(LedgerError::InvalidAuthorizationSet);
    }
    let fee_payer_authorization =
        match (signed.operation.fee_payer(), signed.fee_payer_authorization) {
            (None, None) => None,
            (Some(expected), Some(authorization)) if authorization.signer == expected => {
                Some(authorization)
            }
            _ => return Err(LedgerError::InvalidAuthorizationSet),
        };
    let mut messages = vec![(
        signed.sender_authorization,
        signed
            .operation
            .authorization_message(AuthorizationRole::Sender)?,
    )];
    if let Some(authorization) = fee_payer_authorization {
        messages.push((
            authorization,
            signed
                .operation
                .authorization_message(AuthorizationRole::FeePayer)?,
        ));
    }
    Ok(PreparedOperation {
        network,
        signed,
        messages,
    })
}

fn finalize_prepared(
    operation: Result<PreparedOperation, LedgerError>,
) -> Result<VerifiedOperation, LedgerError> {
    operation.map(PreparedOperation::into_verified)
}

impl Ledger {
    /// Verifies all required signatures and executes the authorized operation.
    ///
    /// This is the transaction-ingress boundary for a future node or API. The
    /// lower-level transfer methods remain the trusted-runtime boundary after
    /// an origin has already been authenticated.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, malformed authorization set,
    /// invalid signature, invalid canonical payload, or ledger rule failure.
    pub fn submit_signed<V: SignatureVerifier>(
        &mut self,
        verifier: &V,
        signed: SignedOperation,
    ) -> Result<OperationReceipt, LedgerError> {
        let checked = signed.verify_for_network(self.network(), verifier)?;
        self.submit_verified(checked)
    }

    /// Executes an operation after its complete authorization set was checked.
    ///
    /// This boundary lets runtimes verify independent signatures concurrently
    /// while monetary state changes remain sequential and deterministic. A
    /// capability is still network-bound and cannot be constructed directly.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability belongs to another network or any
    /// monetary ledger invariant rejects the operation.
    pub fn submit_verified(
        &mut self,
        verified: VerifiedOperation,
    ) -> Result<OperationReceipt, LedgerError> {
        if verified.network != self.network() {
            return Err(LedgerError::WrongNetwork);
        }
        let signed = verified.signed;
        match signed.operation {
            AuthorizedOperation::Transfer(transfer) => {
                self.transfer(signed.sender_authorization.signer, transfer)
            }
            AuthorizedOperation::TransferBatch(batch) => {
                self.transfer_batch(signed.sender_authorization.signer, batch)
            }
            AuthorizedOperation::SponsoredTransfer {
                transfer,
                fee_payer,
            } => self.transfer_sponsored(
                signed.sender_authorization.signer,
                fee_payer,
                fee_payer,
                transfer,
            ),
            AuthorizedOperation::SponsoredBatchTransfer { batch, fee_payer } => self
                .transfer_batch_sponsored(
                    signed.sender_authorization.signer,
                    fee_payer,
                    fee_payer,
                    batch,
                ),
            AuthorizedOperation::ContractDeploy(deploy) => {
                self.deploy_contract(signed.sender_authorization.signer, deploy)
            }
            AuthorizedOperation::ContractCall(call) => {
                self.call_contract(signed.sender_authorization.signer, call)
            }
        }
    }

    /// Executes or deterministically expires an authorized operation at the
    /// supplied consensus block height.
    ///
    /// An expired operation transfers no value or fee, but consumes its exact
    /// sender nonce and emits a finalized expired receipt. This prevents a
    /// reserved account nonce from becoming a permanent gap.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, invalid nonce, arithmetic failure
    /// or any ordinary monetary invariant when the operation is still valid.
    pub fn submit_verified_at_height(
        &mut self,
        verified: VerifiedOperation,
        block_height: u64,
    ) -> Result<OperationReceipt, LedgerError> {
        if verified.network != self.network() {
            return Err(LedgerError::WrongNetwork);
        }
        if block_height > verified.operation().valid_until_height() {
            return self.expire_authorized(verified.operation(), block_height);
        }
        self.submit_verified(verified)
    }
}

fn verify_authorizations<V: SignatureVerifier>(
    verifier: &V,
    signed: &SignedOperation,
) -> Result<(), LedgerError> {
    if signed.sender_authorization.signer != signed.operation.sender() {
        return Err(LedgerError::InvalidAuthorizationSet);
    }
    let fee_payer_authorization =
        match (signed.operation.fee_payer(), signed.fee_payer_authorization) {
            (None, None) => None,
            (Some(expected), Some(authorization)) if authorization.signer == expected => {
                Some(authorization)
            }
            _ => return Err(LedgerError::InvalidAuthorizationSet),
        };

    let sender_message = signed
        .operation
        .authorization_message(AuthorizationRole::Sender)?;
    if !verifier.verify(
        signed.sender_authorization.signer,
        &sender_message,
        signed.sender_authorization.signature,
    ) {
        return Err(LedgerError::InvalidSignature);
    }

    let Some(authorization) = fee_payer_authorization else {
        return Ok(());
    };
    let message = signed
        .operation
        .authorization_message(AuthorizationRole::FeePayer)?;
    if verifier.verify(authorization.signer, &message, authorization.signature) {
        Ok(())
    } else {
        Err(LedgerError::InvalidSignature)
    }
}

const fn role_discriminant(role: AuthorizationRole) -> u8 {
    match role {
        AuthorizationRole::Sender => 0,
        AuthorizationRole::FeePayer => 1,
    }
}

fn encode_transfer(output: &mut Vec<u8>, transfer: Transfer) {
    output.extend_from_slice(transfer.network.as_bytes());
    output.extend_from_slice(transfer.idempotency_key.as_bytes());
    output.extend_from_slice(transfer.asset.as_bytes());
    output.extend_from_slice(transfer.from.as_bytes());
    output.extend_from_slice(transfer.to.as_bytes());
    output.extend_from_slice(&transfer.amount.to_be_bytes());
    output.extend_from_slice(&transfer.fee.to_be_bytes());
    output.extend_from_slice(&transfer.nonce.to_be_bytes());
    output.extend_from_slice(&transfer.valid_until_height.to_be_bytes());
}

fn encode_batch(output: &mut Vec<u8>, batch: &TransferBatch) -> Result<(), LedgerError> {
    if batch.items.len() > MAX_BATCH_ITEMS {
        return Err(LedgerError::BatchTooLarge);
    }
    output.extend_from_slice(batch.network.as_bytes());
    output.extend_from_slice(batch.idempotency_key.as_bytes());
    output.extend_from_slice(batch.asset.as_bytes());
    output.extend_from_slice(batch.from.as_bytes());
    let item_count = u32::try_from(batch.items.len()).map_err(|_| LedgerError::BatchTooLarge)?;
    output.extend_from_slice(&item_count.to_be_bytes());
    for item in &batch.items {
        encode_item(output, *item);
    }
    output.extend_from_slice(&batch.fee.to_be_bytes());
    output.extend_from_slice(&batch.nonce.to_be_bytes());
    output.extend_from_slice(&batch.valid_until_height.to_be_bytes());
    Ok(())
}

fn encode_contract_deploy(
    output: &mut Vec<u8>,
    deploy: &ContractDeploy,
) -> Result<(), LedgerError> {
    if deploy.code.is_empty() || deploy.code.len() > MAX_CONTRACT_CODE_BYTES {
        return Err(LedgerError::InvalidContractCode);
    }
    output.extend_from_slice(deploy.network.as_bytes());
    output.extend_from_slice(deploy.idempotency_key.as_bytes());
    output.extend_from_slice(deploy.asset.as_bytes());
    output.extend_from_slice(deploy.owner.as_bytes());
    output.extend_from_slice(&deploy.salt);
    let length = u32::try_from(deploy.code.len()).map_err(|_| LedgerError::InvalidContractCode)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&deploy.code);
    output.extend_from_slice(&deploy.fee.to_be_bytes());
    output.extend_from_slice(&deploy.nonce.to_be_bytes());
    output.extend_from_slice(&deploy.valid_until_height.to_be_bytes());
    Ok(())
}

fn encode_contract_call(output: &mut Vec<u8>, call: &ContractCall) -> Result<(), LedgerError> {
    if call.entrypoint.is_empty()
        || call.entrypoint.len() > 32
        || call.args.len() > MAX_CONTRACT_ARGS_BYTES
    {
        return Err(LedgerError::InvalidContractArguments);
    }
    output.extend_from_slice(call.network.as_bytes());
    output.extend_from_slice(call.idempotency_key.as_bytes());
    output.extend_from_slice(call.asset.as_bytes());
    output.extend_from_slice(call.caller.as_bytes());
    output.extend_from_slice(call.contract.as_bytes());
    let name_length =
        u8::try_from(call.entrypoint.len()).map_err(|_| LedgerError::InvalidContractArguments)?;
    output.push(name_length);
    output.extend_from_slice(call.entrypoint.as_bytes());
    let args_length =
        u32::try_from(call.args.len()).map_err(|_| LedgerError::InvalidContractArguments)?;
    output.extend_from_slice(&args_length.to_be_bytes());
    output.extend_from_slice(&call.args);
    output.extend_from_slice(&call.attached_amount.to_be_bytes());
    output.extend_from_slice(&call.fee.to_be_bytes());
    output.extend_from_slice(&call.execution_limit.to_be_bytes());
    output.extend_from_slice(&call.nonce.to_be_bytes());
    output.extend_from_slice(&call.valid_until_height.to_be_bytes());
    Ok(())
}

fn encode_item(output: &mut Vec<u8>, item: TransferItem) {
    output.extend_from_slice(item.to.as_bytes());
    output.extend_from_slice(&item.amount.to_be_bytes());
}
