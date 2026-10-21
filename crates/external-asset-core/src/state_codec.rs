use ledger_core::{AccountId, AssetId};

use crate::{
    ConnectorStatus, DepositId, DepositRecord, DepositStatus, DestinationCommitment,
    ExternalAssetConfig, ExternalAssetCoordinator, ExternalAssetSnapshot,
    ExternalAssetStateCodecError, FinalityEvidence, OriginAssetId, OriginDeposit, OriginNetworkId,
    OriginReleaseReference, OriginTransactionId, SettlementReference, WithdrawalId,
    WithdrawalRecord, WithdrawalRequest, WithdrawalStatus, state_decoder::Decoder,
};

const STATE_DOMAIN: &[u8; 24] = b"external.asset.state.v1\0";
const MAX_STATE_ENTRIES: usize = 1_000_000;
pub const MAX_EXTERNAL_ASSET_STATE_BYTES: usize = 256 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Default)]
pub struct ExternalAssetStateCodec;

impl ExternalAssetStateCodec {
    /// Encodes a fully validated canonical external-asset snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid snapshot state, entry bounds or output size.
    pub fn encode(
        snapshot: &ExternalAssetSnapshot,
    ) -> Result<Vec<u8>, ExternalAssetStateCodecError> {
        let canonical = ExternalAssetCoordinator::from_snapshot(snapshot.clone())?.snapshot();
        require_count(canonical.assets.len())?;
        require_count(canonical.deposits.len())?;
        require_count(canonical.withdrawals.len())?;
        let mut output = Vec::new();
        output.extend_from_slice(STATE_DOMAIN);
        output.extend_from_slice(canonical.registry_authority.as_bytes());
        encode_count(&mut output, canonical.assets.len())?;
        for config in canonical.assets {
            encode_config(&mut output, config);
        }
        encode_count(&mut output, canonical.deposits.len())?;
        for deposit in &canonical.deposits {
            encode_deposit(&mut output, deposit);
        }
        encode_count(&mut output, canonical.withdrawals.len())?;
        for withdrawal in &canonical.withdrawals {
            encode_withdrawal(&mut output, withdrawal);
        }
        if output.len() > MAX_EXTERNAL_ASSET_STATE_BYTES {
            return Err(ExternalAssetStateCodecError::StateTooLarge);
        }
        Ok(output)
    }

    /// Decodes and validates complete canonical external-asset state.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, truncated, ambiguous, non-canonical or
    /// invariant-violating input.
    pub fn decode(input: &[u8]) -> Result<ExternalAssetSnapshot, ExternalAssetStateCodecError> {
        if input.len() > MAX_EXTERNAL_ASSET_STATE_BYTES {
            return Err(ExternalAssetStateCodecError::StateTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_array::<24>()? != *STATE_DOMAIN {
            return Err(ExternalAssetStateCodecError::InvalidDomain);
        }
        let registry_authority = AccountId::new(decoder.read_array()?);
        let assets = decode_entries(&mut decoder, decode_config)?;
        let deposits = decode_entries(&mut decoder, decode_deposit)?;
        let withdrawals = decode_entries(&mut decoder, decode_withdrawal)?;
        decoder.finish()?;
        ExternalAssetCoordinator::from_snapshot(ExternalAssetSnapshot {
            registry_authority,
            assets,
            deposits,
            withdrawals,
        })
        .map(|coordinator| coordinator.snapshot())
        .map_err(Into::into)
    }
}

fn encode_config(output: &mut Vec<u8>, config: ExternalAssetConfig) {
    output.extend_from_slice(config.asset.as_bytes());
    output.extend_from_slice(config.origin_network.as_bytes());
    output.extend_from_slice(config.origin_asset.as_bytes());
    output.extend_from_slice(config.observer_authority.as_bytes());
    output.extend_from_slice(config.finality_authority.as_bytes());
    output.extend_from_slice(config.settlement_authority.as_bytes());
    output.extend_from_slice(config.release_authority.as_bytes());
    output.push(match config.status {
        ConnectorStatus::Active => 0,
        ConnectorStatus::DepositsPaused => 1,
        ConnectorStatus::WithdrawalsPaused => 2,
        ConnectorStatus::Suspended => 3,
        ConnectorStatus::Retired => 4,
    });
}

fn decode_config(
    decoder: &mut Decoder<'_>,
) -> Result<ExternalAssetConfig, ExternalAssetStateCodecError> {
    Ok(ExternalAssetConfig {
        asset: AssetId::new(decoder.read_array()?),
        origin_network: OriginNetworkId::new(decoder.read_array()?),
        origin_asset: OriginAssetId::new(decoder.read_array()?),
        observer_authority: AccountId::new(decoder.read_array()?),
        finality_authority: AccountId::new(decoder.read_array()?),
        settlement_authority: AccountId::new(decoder.read_array()?),
        release_authority: AccountId::new(decoder.read_array()?),
        status: match decoder.read_u8()? {
            0 => ConnectorStatus::Active,
            1 => ConnectorStatus::DepositsPaused,
            2 => ConnectorStatus::WithdrawalsPaused,
            3 => ConnectorStatus::Suspended,
            4 => ConnectorStatus::Retired,
            _ => return Err(ExternalAssetStateCodecError::InvalidValue),
        },
    })
}

fn encode_deposit(output: &mut Vec<u8>, record: &DepositRecord) {
    output.extend_from_slice(record.id.as_bytes());
    output.extend_from_slice(record.claim.asset.as_bytes());
    output.extend_from_slice(record.claim.origin_network.as_bytes());
    output.extend_from_slice(record.claim.origin_asset.as_bytes());
    output.extend_from_slice(record.claim.transaction.as_bytes());
    output.extend_from_slice(&record.claim.event_index.to_be_bytes());
    output.extend_from_slice(record.claim.recipient.as_bytes());
    output.extend_from_slice(&record.claim.amount.to_be_bytes());
    output.push(match record.status {
        DepositStatus::Observed => 0,
        DepositStatus::Finalized => 1,
        DepositStatus::MintAuthorized => 2,
        DepositStatus::Minted => 3,
    });
    encode_finality(output, record.finality);
    encode_optional_bytes(output, record.mint_reference.map(|value| *value.as_bytes()));
}

fn decode_deposit(
    decoder: &mut Decoder<'_>,
) -> Result<DepositRecord, ExternalAssetStateCodecError> {
    let id = DepositId::new(decoder.read_array()?);
    let claim = OriginDeposit {
        asset: AssetId::new(decoder.read_array()?),
        origin_network: OriginNetworkId::new(decoder.read_array()?),
        origin_asset: OriginAssetId::new(decoder.read_array()?),
        transaction: OriginTransactionId::new(decoder.read_array()?),
        event_index: decoder.read_u32()?,
        recipient: AccountId::new(decoder.read_array()?),
        amount: decoder.read_u128()?,
    };
    let status = match decoder.read_u8()? {
        0 => DepositStatus::Observed,
        1 => DepositStatus::Finalized,
        2 => DepositStatus::MintAuthorized,
        3 => DepositStatus::Minted,
        _ => return Err(ExternalAssetStateCodecError::InvalidValue),
    };
    Ok(DepositRecord {
        id,
        claim,
        status,
        finality: decode_finality(decoder)?,
        mint_reference: decode_optional_bytes(decoder)?.map(SettlementReference::new),
    })
}

fn encode_withdrawal(output: &mut Vec<u8>, record: &WithdrawalRecord) {
    output.extend_from_slice(record.id.as_bytes());
    output.extend_from_slice(record.request.asset.as_bytes());
    output.extend_from_slice(record.request.account.as_bytes());
    output.extend_from_slice(&record.request.amount.to_be_bytes());
    output.extend_from_slice(record.request.destination.as_bytes());
    output.extend_from_slice(record.request.burn_reference.as_bytes());
    output.push(match record.status {
        WithdrawalStatus::BurnVerified => 0,
        WithdrawalStatus::ReleaseAuthorized => 1,
        WithdrawalStatus::Submitted => 2,
        WithdrawalStatus::Completed => 3,
    });
    match record.origin_release {
        None => output.push(0),
        Some(reference) => {
            output.push(1);
            output.extend_from_slice(reference.transaction.as_bytes());
            output.extend_from_slice(&reference.event_index.to_be_bytes());
        }
    }
    encode_finality(output, record.finality);
}

fn decode_withdrawal(
    decoder: &mut Decoder<'_>,
) -> Result<WithdrawalRecord, ExternalAssetStateCodecError> {
    let id = WithdrawalId::new(decoder.read_array()?);
    let request = WithdrawalRequest {
        asset: AssetId::new(decoder.read_array()?),
        account: AccountId::new(decoder.read_array()?),
        amount: decoder.read_u128()?,
        destination: DestinationCommitment::new(decoder.read_array()?),
        burn_reference: SettlementReference::new(decoder.read_array()?),
    };
    let status = match decoder.read_u8()? {
        0 => WithdrawalStatus::BurnVerified,
        1 => WithdrawalStatus::ReleaseAuthorized,
        2 => WithdrawalStatus::Submitted,
        3 => WithdrawalStatus::Completed,
        _ => return Err(ExternalAssetStateCodecError::InvalidValue),
    };
    let origin_release = match decoder.read_u8()? {
        0 => None,
        1 => Some(OriginReleaseReference {
            transaction: OriginTransactionId::new(decoder.read_array()?),
            event_index: decoder.read_u32()?,
        }),
        _ => return Err(ExternalAssetStateCodecError::InvalidValue),
    };
    Ok(WithdrawalRecord {
        id,
        request,
        status,
        origin_release,
        finality: decode_finality(decoder)?,
    })
}

fn encode_finality(output: &mut Vec<u8>, evidence: Option<FinalityEvidence>) {
    match evidence {
        None => output.push(0),
        Some(evidence) => {
            output.push(1);
            output.extend_from_slice(&evidence.block_height.to_be_bytes());
            output.extend_from_slice(&evidence.block_hash);
            output.extend_from_slice(&evidence.proof_hash);
        }
    }
}

fn decode_finality(
    decoder: &mut Decoder<'_>,
) -> Result<Option<FinalityEvidence>, ExternalAssetStateCodecError> {
    match decoder.read_u8()? {
        0 => Ok(None),
        1 => Ok(Some(FinalityEvidence {
            block_height: decoder.read_u64()?,
            block_hash: decoder.read_array()?,
            proof_hash: decoder.read_array()?,
        })),
        _ => Err(ExternalAssetStateCodecError::InvalidValue),
    }
}

fn encode_optional_bytes(output: &mut Vec<u8>, value: Option<[u8; 32]>) {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            output.extend_from_slice(&value);
        }
    }
}

fn decode_optional_bytes(
    decoder: &mut Decoder<'_>,
) -> Result<Option<[u8; 32]>, ExternalAssetStateCodecError> {
    match decoder.read_u8()? {
        0 => Ok(None),
        1 => decoder.read_array().map(Some),
        _ => Err(ExternalAssetStateCodecError::InvalidValue),
    }
}

fn decode_entries<T>(
    decoder: &mut Decoder<'_>,
    decode: fn(&mut Decoder<'_>) -> Result<T, ExternalAssetStateCodecError>,
) -> Result<Vec<T>, ExternalAssetStateCodecError> {
    let count = usize::try_from(decoder.read_u32()?)
        .map_err(|_| ExternalAssetStateCodecError::TooManyEntries)?;
    require_count(count)?;
    if count > decoder.remaining() {
        return Err(ExternalAssetStateCodecError::TooManyEntries);
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(decode(decoder)?);
    }
    Ok(entries)
}

fn encode_count(output: &mut Vec<u8>, count: usize) -> Result<(), ExternalAssetStateCodecError> {
    require_count(count)?;
    let count = u32::try_from(count).map_err(|_| ExternalAssetStateCodecError::TooManyEntries)?;
    output.extend_from_slice(&count.to_be_bytes());
    Ok(())
}

fn require_count(count: usize) -> Result<(), ExternalAssetStateCodecError> {
    if count > MAX_STATE_ENTRIES {
        Err(ExternalAssetStateCodecError::TooManyEntries)
    } else {
        Ok(())
    }
}
