use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAssetView {
    pub id: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatusView {
    pub network_id: String,
    pub address_prefix: String,
    pub finalized_height: String,
    pub finality_mode: &'static str,
    pub asset: NetworkAssetView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStateView {
    pub address: String,
    pub account_id: String,
    pub nonce: String,
    pub balance: String,
    pub finalized_height: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedTransactionView {
    pub id: String,
    pub block_height: String,
    pub operation_index: String,
    pub from: String,
    pub to: String,
    pub amount: String,
    pub fee: String,
    pub outcome: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedBlockView {
    pub height: String,
    pub hash: String,
    pub state_root: String,
    pub transaction_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExplorerOverviewView {
    pub status: NetworkStatusView,
    pub blocks: Vec<FinalizedBlockView>,
    pub transactions: Vec<FinalizedTransactionView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SubmissionResultView {
    pub transaction: FinalizedTransactionView,
    pub checkpoint: FinalizedBlockView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutView {
    pub id: String,
    pub merchant_label: &'static str,
    pub merchant_address: String,
    pub amount: String,
    pub fee: String,
    pub asset: NetworkAssetView,
    pub status: &'static str,
    pub expires_at_ms: String,
    pub valid_until_height: String,
    pub payment_path: String,
    pub sms_text: String,
    pub transaction: Option<FinalizedTransactionView>,
}

#[derive(Debug, Deserialize)]
pub struct FaucetRequest {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub envelope: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCheckoutRequest {
    pub merchant_address: String,
    pub amount: String,
    pub order_reference: String,
}

#[derive(Serialize)]
pub struct ErrorView {
    pub error: String,
}
