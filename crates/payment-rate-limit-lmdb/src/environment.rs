use std::path::Path;

use heed::{Env, EnvOpenOptions};

use crate::{LmdbRateLimitError, LmdbRateLimitOptions};

const MAX_DATABASES: u32 = 5;

#[allow(unsafe_code)]
pub(crate) fn open_environment(
    path: &Path,
    options: LmdbRateLimitOptions,
) -> Result<Env, LmdbRateLimitError> {
    let mut builder = EnvOpenOptions::new();
    builder
        .map_size(options.map_size)
        .max_dbs(MAX_DATABASES)
        .max_readers(options.max_readers);
    // SAFETY: the canonical service-owned directory is local, is never
    // modified outside LMDB while mapped, retains LMDB locking/full durability,
    // and mapped references never escape their transaction lifetime.
    unsafe { builder.open(path) }.map_err(Into::into)
}
