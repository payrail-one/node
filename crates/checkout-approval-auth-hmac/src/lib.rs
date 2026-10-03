#![forbid(unsafe_code)]

use checkout_approval_core::{ApprovalCode, ApprovalCodeAuthenticator, ApprovalCodeDigest};
use hmac::{Hmac, Mac};
use ledger_core::NetworkId;
use sha2::Sha256;

const DIGEST_DOMAIN: &[u8] = b"checkout.approval.code.v1\0";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidApprovalCodeKey;

#[derive(Clone)]
pub struct HmacApprovalCodeAuthenticator {
    template: HmacSha256,
}

impl HmacApprovalCodeAuthenticator {
    /// Initializes the authenticator with a deployment secret. Production keys
    /// must come from a secret manager and be independently rotated per network.
    ///
    /// # Errors
    ///
    /// Returns an error when the HMAC implementation rejects the key length.
    pub fn new(key: &[u8]) -> Result<Self, InvalidApprovalCodeKey> {
        if key.len() < 32 {
            return Err(InvalidApprovalCodeKey);
        }
        let template = HmacSha256::new_from_slice(key).map_err(|_| InvalidApprovalCodeKey)?;
        Ok(Self { template })
    }
}

impl ApprovalCodeAuthenticator for HmacApprovalCodeAuthenticator {
    fn digest(&self, network: NetworkId, code: ApprovalCode) -> ApprovalCodeDigest {
        let mut mac = self.template.clone();
        mac.update(DIGEST_DOMAIN);
        mac.update(network.as_bytes());
        mac.update(code.as_bytes());
        ApprovalCodeDigest::new(mac.finalize().into_bytes().into())
    }
}

#[cfg(test)]
mod tests {
    use checkout_approval_core::{ApprovalCode, ApprovalCodeAuthenticator};
    use ledger_core::NetworkId;

    use super::HmacApprovalCodeAuthenticator;

    #[test]
    fn digest_is_bound_to_code_network_and_secret() {
        let first = HmacApprovalCodeAuthenticator::new(&[11; 32]).unwrap();
        let second = HmacApprovalCodeAuthenticator::new(&[12; 32]).unwrap();
        let code = ApprovalCode::parse("123456").unwrap();
        let same = first.digest(NetworkId::new([1; 32]), code);

        assert_eq!(same, first.digest(NetworkId::new([1; 32]), code));
        assert_ne!(same, first.digest(NetworkId::new([2; 32]), code));
        assert_ne!(
            same,
            first.digest(
                NetworkId::new([1; 32]),
                ApprovalCode::parse("123457").unwrap()
            )
        );
        assert_ne!(same, second.digest(NetworkId::new([1; 32]), code));
    }

    #[test]
    fn short_secrets_are_rejected() {
        assert!(HmacApprovalCodeAuthenticator::new(&[1; 31]).is_err());
    }
}
