#[derive(serde::Serialize)]
pub struct NewInvoiceRequest {
    pub invoice_id: String,
    pub method: String,
    pub amount_atomic: u64,
    pub token: String,
    pub success_callback: String,
    pub status_update_callback: String,
}

#[derive(serde::Deserialize)]
pub struct PaymentInfoResponse {
    pub amount: Amount,
    pub address: Address,
    pub method: Method,
}

#[derive(serde::Deserialize)]
pub struct Address {
    inner: String,
}
impl Address {
    pub fn from(s: String) -> Self {
        Self {
            inner: s.to_string(),
        }
    }
}
impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}
#[derive(serde::Deserialize)]
pub struct Amount {
    inner: String,
}
impl Amount {
    pub fn from(s: String) -> Self {
        Self {
            inner: s.to_string(),
        }
    }
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }

}
#[derive(serde::Deserialize)]
pub struct Method {
    inner: String,
}
impl Method {
    pub fn from(s: String) -> Self {
        Self {
            inner: s.to_string(),
        }
    }
}
impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}
