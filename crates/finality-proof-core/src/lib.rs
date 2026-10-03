#![forbid(unsafe_code)]

mod certificate;
mod codec;
mod error;
mod set;
mod types;
mod verifier;

pub use certificate::FinalityCertificate;
pub use codec::FinalityCertificateCodec;
pub use error::FinalityError;
pub use set::{MAX_VALIDATORS, ValidatorSet};
pub use types::{
    ConsensusSignature, FinalitySignatureVerifier, Validator, ValidatorSignature, VerifiedFinality,
};
pub use verifier::FinalityVerifier;
