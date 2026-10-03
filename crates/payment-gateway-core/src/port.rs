use ledger_core::{AccountId, AuthorizedOperation, NetworkId, Nonce, SignedOperation};

use crate::CanonicalPaymentRequest;

pub trait AccountNonceSource {
    type Error;

    /// Returns the next nonce implied by finalized ledger state.
    ///
    /// # Errors
    ///
    /// Returns the source error when authoritative state is unavailable.
    fn finalized_next_nonce(
        &self,
        network: NetworkId,
        account: AccountId,
    ) -> Result<Nonce, Self::Error>;
}

pub trait PaymentPolicy {
    type Error;

    /// Authorizes a canonical request before a nonce is allocated.
    ///
    /// # Errors
    ///
    /// Returns the policy error when the payment is denied or policy state is
    /// unavailable.
    fn authorize(&self, request: &CanonicalPaymentRequest) -> Result<(), Self::Error>;
}

pub trait PaymentOperationSigner {
    type Error;

    /// Produces all role-bound authorizations required by the operation.
    ///
    /// # Errors
    ///
    /// Returns the signer error when keys or the signing service are unavailable.
    fn sign(&self, operation: AuthorizedOperation) -> Result<SignedOperation, Self::Error>;
}

pub trait SignedPaymentVerifier {
    type Error;

    /// Applies authoritative cryptographic verification before journaling.
    ///
    /// # Errors
    ///
    /// Returns the verifier error for malformed or invalid authorizations.
    fn verify(&self, signed: &SignedOperation) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationAck {
    Accepted,
    AlreadyKnown,
}

pub trait PaymentPublisher {
    type Error;

    /// Publishes an already verified and durably journaled operation.
    ///
    /// # Errors
    ///
    /// Returns the transport/admission error when publication outcome is not
    /// safely known to the gateway.
    fn publish(&self, signed: &SignedOperation) -> Result<PublicationAck, Self::Error>;
}
