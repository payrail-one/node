use ledger_core::{AccountId, AssetId, NetworkId};
use merchant_checkout_core::{
    Checkout, CheckoutClaim, CheckoutDefinition, CheckoutDispatchState, CheckoutId, CheckoutState,
    FeeMode, MerchantId, MerchantOrderKey,
};
use payment_idempotency_core::{ClientRequestKey, TenantId};
use sha2::{Digest, Sha256};

use crate::MerchantCheckoutStoreError;

const RECORD_MAGIC: [u8; 8] = *b"MCHK0002";
const CHECKSUM_LENGTH: usize = 32;

pub(crate) fn checkout_key(id: CheckoutId) -> [u8; 32] {
    *id.as_bytes()
}

pub(crate) fn decode_checkout_key(
    encoded: &[u8],
) -> Result<CheckoutId, MerchantCheckoutStoreError> {
    Ok(CheckoutId::new(array(encoded)?))
}

pub(crate) fn order_key(definition: CheckoutDefinition) -> [u8; 96] {
    let mut key = [0_u8; 96];
    key[..32].copy_from_slice(definition.tenant.as_bytes());
    key[32..64].copy_from_slice(definition.merchant.as_bytes());
    key[64..].copy_from_slice(definition.order_key.as_bytes());
    key
}

pub(crate) fn encode_record(checkout: &Checkout) -> Vec<u8> {
    let definition = checkout.definition();
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&RECORD_MAGIC);
    encoded.extend_from_slice(definition.network.as_bytes());
    encoded.extend_from_slice(definition.tenant.as_bytes());
    encoded.extend_from_slice(definition.merchant.as_bytes());
    encoded.extend_from_slice(definition.order_key.as_bytes());
    encoded.extend_from_slice(definition.merchant_account.as_bytes());
    encoded.extend_from_slice(definition.asset.as_bytes());
    encoded.extend_from_slice(&definition.amount.to_be_bytes());
    encoded.extend_from_slice(&definition.maximum_fee.to_be_bytes());
    encoded.push(fee_mode(definition.fee_mode));
    encoded.extend_from_slice(&definition.expires_at_ms.to_be_bytes());
    encoded.extend_from_slice(&definition.valid_until_height.to_be_bytes());
    encoded.extend_from_slice(&checkout.created_at_ms().to_be_bytes());
    encoded.extend_from_slice(&checkout.updated_at_ms().to_be_bytes());
    encode_state(&mut encoded, checkout.state());
    let checksum: [u8; 32] = Sha256::digest(&encoded).into();
    encoded.extend_from_slice(&checksum);
    encoded
}

pub(crate) fn decode_record(input: &[u8]) -> Result<Checkout, MerchantCheckoutStoreError> {
    let body_length = input
        .len()
        .checked_sub(CHECKSUM_LENGTH)
        .ok_or(MerchantCheckoutStoreError::CorruptRecord)?;
    let expected: [u8; 32] = Sha256::digest(&input[..body_length]).into();
    if input[body_length..] != expected {
        return Err(MerchantCheckoutStoreError::CorruptRecord);
    }
    let mut decoder = Decoder::new(&input[..body_length]);
    if decoder.array::<8>()? != RECORD_MAGIC {
        return Err(MerchantCheckoutStoreError::CorruptRecord);
    }
    let definition = CheckoutDefinition {
        network: NetworkId::new(decoder.array()?),
        tenant: TenantId::new(decoder.array()?),
        merchant: MerchantId::new(decoder.array()?),
        order_key: MerchantOrderKey::new(decoder.array()?),
        merchant_account: AccountId::new(decoder.array()?),
        asset: AssetId::new(decoder.array()?),
        amount: decoder.u128()?,
        maximum_fee: decoder.u128()?,
        fee_mode: decode_fee_mode(decoder.u8()?)?,
        expires_at_ms: decoder.u64()?,
        valid_until_height: decoder.u64()?,
    };
    let created_at_ms = decoder.u64()?;
    let updated_at_ms = decoder.u64()?;
    let state = decode_state(&mut decoder)?;
    decoder.finish()?;
    Checkout::restore(definition, created_at_ms, updated_at_ms, state).map_err(Into::into)
}

fn encode_state(output: &mut Vec<u8>, state: CheckoutState) {
    match state {
        CheckoutState::Open => output.push(0),
        CheckoutState::Claimed { claim, dispatch } => {
            output.push(1);
            output.extend_from_slice(claim.payer.as_bytes());
            output.extend_from_slice(claim.client_key.as_bytes());
            output.extend_from_slice(&claim.fee.to_be_bytes());
            output.extend_from_slice(&claim.claimed_at_ms.to_be_bytes());
            output.push(match dispatch {
                CheckoutDispatchState::Pending => 0,
                CheckoutDispatchState::GatewayRecorded => 1,
            });
        }
    }
}

fn decode_state(decoder: &mut Decoder<'_>) -> Result<CheckoutState, MerchantCheckoutStoreError> {
    match decoder.u8()? {
        0 => Ok(CheckoutState::Open),
        1 => Ok(CheckoutState::Claimed {
            claim: CheckoutClaim {
                payer: AccountId::new(decoder.array()?),
                client_key: ClientRequestKey::new(decoder.array()?),
                fee: decoder.u128()?,
                claimed_at_ms: decoder.u64()?,
            },
            dispatch: match decoder.u8()? {
                0 => CheckoutDispatchState::Pending,
                1 => CheckoutDispatchState::GatewayRecorded,
                _ => return Err(MerchantCheckoutStoreError::CorruptRecord),
            },
        }),
        _ => Err(MerchantCheckoutStoreError::CorruptRecord),
    }
}

fn fee_mode(value: FeeMode) -> u8 {
    match value {
        FeeMode::CustomerPays => 0,
        FeeMode::MerchantSponsored => 1,
    }
}

fn decode_fee_mode(value: u8) -> Result<FeeMode, MerchantCheckoutStoreError> {
    match value {
        0 => Ok(FeeMode::CustomerPays),
        1 => Ok(FeeMode::MerchantSponsored),
        _ => Err(MerchantCheckoutStoreError::CorruptRecord),
    }
}

struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], MerchantCheckoutStoreError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(MerchantCheckoutStoreError::CorruptRecord)?;
        let value = self
            .input
            .get(self.position..end)
            .ok_or(MerchantCheckoutStoreError::CorruptRecord)?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], MerchantCheckoutStoreError> {
        array(self.bytes(N)?)
    }

    fn u8(&mut self) -> Result<u8, MerchantCheckoutStoreError> {
        self.bytes(1)?
            .first()
            .copied()
            .ok_or(MerchantCheckoutStoreError::CorruptRecord)
    }

    fn u64(&mut self) -> Result<u64, MerchantCheckoutStoreError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn u128(&mut self) -> Result<u128, MerchantCheckoutStoreError> {
        Ok(u128::from_be_bytes(self.array()?))
    }

    fn finish(self) -> Result<(), MerchantCheckoutStoreError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(MerchantCheckoutStoreError::CorruptRecord)
        }
    }
}

fn array<const N: usize>(input: &[u8]) -> Result<[u8; N], MerchantCheckoutStoreError> {
    input
        .try_into()
        .map_err(|_| MerchantCheckoutStoreError::CorruptRecord)
}
