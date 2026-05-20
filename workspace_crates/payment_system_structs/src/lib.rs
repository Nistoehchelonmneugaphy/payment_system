use std::collections::HashMap;

use strum::EnumString;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PaymentSystemConfig {
    pub database_url: String,
    pub secret_prefix: String,
    pub monero_wallet_rpc_url: String,
    pub monero_confirmations: u64,
    // limit monitoring load
    pub max_active_invoices: u64,
    pub active_invoice_minutes_since_creation: u64,
    pub master_seed: String,
    pub cf_bypass_token_header_name: String,
    pub cf_bypass_token_header_value: String,
}


pub struct InvoicePaymentData {
    pub primary_key: i64,
    pub invoice_id: String,
    pub status_update_callback: String,
    pub success_callback: String,
    pub method: String,
    pub generated_address: String,
    pub address_index: String,
    pub amount: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub sweeped_at: Option<chrono::DateTime<chrono::Utc>>,
}


#[derive(Clone, serde::Serialize, serde::Deserialize, EnumString, strum::Display,)]
#[strum(ascii_case_insensitive)]
pub enum PaymentMethod {
    #[strum(serialize = "litecoin", serialize = "ltc")]
    Litecoin,
    #[strum(serialize = "monero", serialize = "xmr")]
    Monero,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NewInvoiceRequest {
    pub invoice_id: String,
    pub method: PaymentMethod,
    pub amount_atomic: u64,
    pub success_callback: String,
    pub status_update_callback: String,
    pub callback_headers: CallbackHeaders,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CallbackHeaders(pub HashMap<String, String>);

/// Sent in response to invoice creation
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PaymentInfo {
    pub amount: Amount,
    pub address: Address,
    pub method: PaymentMethod,
}

#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct Address(pub String);
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct Amount(pub u64);
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct AddressIndex(pub u64);
