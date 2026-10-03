use heed::{Database, RoTxn, RwTxn, types::Bytes};
use ledger_runtime_core::LedgerRow;

use crate::LmdbStateStoreError;

pub(crate) fn read_rows(
    database: Database<Bytes, Bytes>,
    transaction: &RoTxn<'_>,
) -> Result<Vec<LedgerRow>, LmdbStateStoreError> {
    let mut rows = Vec::new();
    for result in database.iter(transaction)? {
        let (key, value) = result?;
        rows.push(LedgerRow {
            key: key.to_vec(),
            value: value.to_vec(),
        });
    }
    Ok(rows)
}

pub(crate) fn write_rows(
    database: Database<Bytes, Bytes>,
    transaction: &mut RwTxn<'_>,
    rows: &[LedgerRow],
) -> Result<(), LmdbStateStoreError> {
    for row in rows {
        database.put(transaction, &row.key, &row.value)?;
    }
    Ok(())
}

pub(crate) fn sync_rows(
    database: Database<Bytes, Bytes>,
    transaction: &mut RwTxn<'_>,
    requested: &[LedgerRow],
) -> Result<(), LmdbStateStoreError> {
    let existing = read_rows(database, transaction)?;
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < existing.len() || new_index < requested.len() {
        match (existing.get(old_index), requested.get(new_index)) {
            (Some(old), Some(new)) if old.key == new.key => {
                if old.value != new.value {
                    database.put(transaction, &new.key, &new.value)?;
                }
                old_index += 1;
                new_index += 1;
            }
            (Some(old), Some(new)) if old.key < new.key => {
                database.delete(transaction, &old.key)?;
                old_index += 1;
            }
            (Some(_) | None, Some(new)) => {
                database.put(transaction, &new.key, &new.value)?;
                new_index += 1;
            }
            (Some(old), None) => {
                database.delete(transaction, &old.key)?;
                old_index += 1;
            }
            (None, None) => break,
        }
    }
    Ok(())
}
