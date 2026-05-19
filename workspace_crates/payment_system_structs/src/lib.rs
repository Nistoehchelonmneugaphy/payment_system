
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
}

/// Sent in response to invoice creation
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PaymentInfo {
    pub amount: Amount,
    pub address: Address,
    pub method: Method,
}


// let's play around with macro_rules!

macro_rules! string_newtype {
    ($($name:ident),*) => {
        $(
            #[derive(serde::Serialize, serde::Deserialize)]
            pub struct $name {
                inner: String,
            }

            impl $name {
                pub fn from(s: impl Into<String>) -> Self {
                    Self { inner: s.into() }
                }
            }

            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.inner)
                }
            }
        )*
    };
}
string_newtype!(Address, Amount, Method);
