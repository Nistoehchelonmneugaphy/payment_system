#[derive(serde::Serialize)]
pub struct NewInvoiceRequest {
    pub invoice_id: String,
    pub method: String,
    pub amount_atomic: u64,
    pub token: String,
}

#[derive(serde::Deserialize)]
pub struct NewInvoiceResponse {
    pub address: String,
    pub amount: String,
    pub method: String,
}