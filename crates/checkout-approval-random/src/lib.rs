#![forbid(unsafe_code)]

use checkout_approval_core::{ApprovalCode, ApprovalCodeGenerator};

const CODE_SPACE: u64 = 1_000_000;
const U32_SPACE: u64 = 1_u64 << 32;
const UNBIASED_ZONE: u64 = (U32_SPACE / CODE_SPACE) * CODE_SPACE;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SystemApprovalCodeGenerator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalCodeRandomError;

impl ApprovalCodeGenerator for SystemApprovalCodeGenerator {
    type Error = ApprovalCodeRandomError;

    /// Generates all one-million six-digit values without modulo bias using the
    /// operating system CSPRNG. Leading zeroes are preserved.
    ///
    /// # Errors
    ///
    /// Returns an error if the operating system random source fails.
    fn generate(&self) -> Result<ApprovalCode, Self::Error> {
        loop {
            let mut bytes = [0_u8; 4];
            getrandom::fill(&mut bytes).map_err(|_| ApprovalCodeRandomError)?;
            let sample = u64::from(u32::from_be_bytes(bytes));
            if sample < UNBIASED_ZONE {
                let number =
                    u32::try_from(sample % CODE_SPACE).map_err(|_| ApprovalCodeRandomError)?;
                return ApprovalCode::from_number(number).map_err(|_| ApprovalCodeRandomError);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use checkout_approval_core::ApprovalCodeGenerator;

    use super::SystemApprovalCodeGenerator;

    #[test]
    fn system_generator_returns_six_ascii_digits() {
        let generator = SystemApprovalCodeGenerator;
        for _ in 0..1_000 {
            let code = generator.generate().unwrap();
            assert_eq!(code.as_bytes().len(), 6);
            assert!(code.as_bytes().iter().all(u8::is_ascii_digit));
        }
    }
}
