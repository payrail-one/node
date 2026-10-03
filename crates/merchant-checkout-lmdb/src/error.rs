use merchant_checkout_core::CheckoutError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MerchantCheckoutStoreError {
    Domain(CheckoutError),
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    CheckoutNotFound,
    CorruptRecord,
}

impl From<CheckoutError> for MerchantCheckoutStoreError {
    fn from(error: CheckoutError) -> Self {
        Self::Domain(error)
    }
}

impl From<heed::Error> for MerchantCheckoutStoreError {
    fn from(error: heed::Error) -> Self {
        match error {
            heed::Error::Mdb(heed::MdbError::MapFull) => Self::MapFull,
            heed::Error::Mdb(
                heed::MdbError::PageNotFound
                | heed::MdbError::Corrupted
                | heed::MdbError::Panic
                | heed::MdbError::VersionMismatch
                | heed::MdbError::Invalid,
            ) => Self::CorruptRecord,
            _ => Self::Database,
        }
    }
}

impl From<std::io::Error> for MerchantCheckoutStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
