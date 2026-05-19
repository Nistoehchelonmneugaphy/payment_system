use std::collections::HashMap;

use sqlx::PgConnection;
use strum::EnumString;

#[derive(Clone, serde::Serialize, serde::Deserialize, EnumString, strum::Display)]
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

impl CallbackHeaders {
    pub async fn fetch(conn: &mut PgConnection, invoice_id: &str) -> Result<Self, String> {
        let Ok(rows) = sqlx::query!(
            "SELECT key, value FROM callback_headers WHERE invoice_id = $1",
            invoice_id
        )
        .fetch_all(conn)
        .await else {
            return Err(format!("DB err"));
        };

        let map = rows.into_iter().map(|r| (r.key, r.value)).collect();
        Ok(Self(map))
    }
}

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
pub struct Amount(pub String);
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
