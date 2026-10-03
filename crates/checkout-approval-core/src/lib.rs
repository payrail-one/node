#![forbid(unsafe_code)]

mod error;
mod model;
mod port;
mod service;

pub use error::{ApprovalCodeError, ApprovalCodeServiceError};
pub use model::{
    ApprovalCode, ApprovalCodeClaim, ApprovalCodeDigest, ApprovalCodeMutation,
    ApprovalCodeMutationOutcome, ApprovalCodePolicy, ApprovalCodeRecord, ApprovalCodeState,
    IssuedApprovalCode,
};
pub use port::{ApprovalCodeAuthenticator, ApprovalCodeGenerator, ApprovalCodeStore};
pub use service::ApprovalCodeService;
