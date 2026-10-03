use std::path::Path;

use heed::{Env, EnvOpenOptions};

use crate::{MerchantCheckoutStoreError, MerchantCheckoutStoreOptions};

const MAX_DATABASES: u32 = 6;

#[allow(unsafe_code)]
pub(crate) fn open_environment(
    path: &Path,
    options: MerchantCheckoutStoreOptions,
) -> Result<Env, MerchantCheckoutStoreError> {
    let mut builder = EnvOpenOptions::new();
    builder
        .map_size(options.map_size)
        .max_dbs(MAX_DATABASES)
        .max_readers(options.max_readers);
    // SAFETY: the canonical service-owned directory is not modified outside
    // LMDB while mapped. Locking and full durability remain enabled, and mapped
    // references never escape their transaction lifetime.
    unsafe { builder.open(path) }.map_err(Into::into)
}
