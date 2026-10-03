use std::time::{SystemTime, UNIX_EPOCH};

use account_address::AddressCodec;
use ledger_core::Transfer;
use ledger_core::{AccountId, AuthorizedOperation, IdempotencyKey, SignedOperation};
use merchant_checkout_core::{
    Checkout, CheckoutDefinition, CheckoutId, CheckoutState, FeeMode, MerchantId, MerchantOrderKey,
};
use payment_idempotency_core::TenantId;
use sha2::{Digest, Sha256};

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::{CheckoutView, FinalizedTransactionView, NetworkAssetView},
    service::{ASSET, NETWORK},
};

pub const DEVNET_TENANT: TenantId = TenantId::new([71; 32]);
const CHECKOUT_LIFETIME_MS: u64 = 15 * 60 * 1_000;
const CHECKOUT_VALIDITY_BLOCKS: u64 = 100;
const MERCHANT_LABEL: &str = "Aurora Market · Devnet";
const MERCHANT_ID_DOMAIN: &[u8] = b"devnet.merchant\0";
const ORDER_KEY_DOMAIN: &[u8] = b"devnet.order\0";

pub fn create_definition(
    merchant: AccountId,
    amount: &str,
    order_reference: &str,
    finalized_height: u64,
    now_ms: u64,
) -> Result<CheckoutDefinition, DevnetError> {
    let amount = amount
        .parse::<u128>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(DevnetError::InvalidCheckout)?;
    validate_order_reference(order_reference)?;
    Ok(CheckoutDefinition {
        network: NETWORK,
        tenant: DEVNET_TENANT,
        merchant: MerchantId::new(hash(MERCHANT_ID_DOMAIN, merchant.as_bytes())),
        order_key: MerchantOrderKey::new(hash(ORDER_KEY_DOMAIN, order_reference.as_bytes())),
        merchant_account: merchant,
        asset: ASSET,
        amount,
        maximum_fee: 0,
        fee_mode: FeeMode::CustomerPays,
        expires_at_ms: now_ms
            .checked_add(CHECKOUT_LIFETIME_MS)
            .ok_or(DevnetError::InternalInvariant)?,
        valid_until_height: finalized_height
            .checked_add(CHECKOUT_VALIDITY_BLOCKS)
            .ok_or(DevnetError::InternalInvariant)?,
    })
}

pub fn parse_id(value: &str) -> Result<CheckoutId, DevnetError> {
    let bytes = decode_bounded_hex(value, 32).map_err(|_| DevnetError::InvalidCheckout)?;
    let value: [u8; 32] = bytes.try_into().map_err(|_| DevnetError::InvalidCheckout)?;
    Ok(CheckoutId::new(value))
}

pub fn payment_idempotency(id: CheckoutId) -> IdempotencyKey {
    IdempotencyKey::new(*id.as_bytes())
}

pub fn validate_payment(
    checkout: &Checkout,
    signed: &SignedOperation,
) -> Result<AccountId, DevnetError> {
    let AuthorizedOperation::Transfer(transfer) = signed.operation else {
        return Err(DevnetError::InvalidCheckout);
    };
    validate_transfer(checkout, transfer)?;
    Ok(transfer.from)
}

pub fn validate_transfer(checkout: &Checkout, transfer: Transfer) -> Result<(), DevnetError> {
    let definition = checkout.definition();
    if transfer.network != definition.network
        || transfer.asset != definition.asset
        || transfer.to != definition.merchant_account
        || transfer.amount != definition.amount
        || transfer.fee > definition.maximum_fee
        || transfer.valid_until_height != definition.valid_until_height
        || transfer.idempotency_key != payment_idempotency(checkout.id())
    {
        return Err(DevnetError::InvalidCheckout);
    }
    Ok(())
}

pub fn view(
    checkout: &Checkout,
    addresses: &AddressCodec,
    asset: NetworkAssetView,
    settled: Option<FinalizedTransactionView>,
    now_ms: u64,
) -> Result<CheckoutView, DevnetError> {
    let definition = checkout.definition();
    let id = encode_hex(checkout.id().as_bytes());
    let status = if settled.is_some() {
        "finalized"
    } else if now_ms >= definition.expires_at_ms {
        "expired"
    } else if matches!(checkout.state(), CheckoutState::Claimed { .. }) {
        "processing"
    } else {
        "open"
    };
    let payment_path = format!("/pay/{id}");
    let sms_text = format!(
        "Payment request: {} {}. Open {{origin}}{payment_path}. A link never authorizes payment.",
        display_amount(definition.amount),
        asset.symbol
    );
    Ok(CheckoutView {
        id,
        merchant_label: MERCHANT_LABEL,
        merchant_address: addresses
            .encode(definition.merchant_account)
            .map_err(|_| DevnetError::InternalInvariant)?,
        amount: definition.amount.to_string(),
        fee: "0".to_owned(),
        asset,
        status,
        expires_at_ms: definition.expires_at_ms.to_string(),
        valid_until_height: definition.valid_until_height.to_string(),
        sms_text,
        payment_path,
        transaction: settled,
    })
}

pub fn now_ms() -> Result<u64, DevnetError> {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DevnetError::InternalInvariant)?
        .as_millis();
    u64::try_from(milliseconds).map_err(|_| DevnetError::InternalInvariant)
}

fn validate_order_reference(value: &str) -> Result<(), DevnetError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DevnetError::InvalidCheckout);
    }
    Ok(())
}

fn hash(domain: &[u8], value: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    digest.finalize().into()
}

fn display_amount(amount: u128) -> String {
    const SCALE: u128 = 1_000_000;
    let whole = amount / SCALE;
    let fraction = amount % SCALE;
    if fraction == 0 {
        return whole.to_string();
    }
    let padded = format!("{fraction:06}");
    format!("{whole}.{}", padded.trim_end_matches('0'))
}
