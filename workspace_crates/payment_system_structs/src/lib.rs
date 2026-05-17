#[derive(serde::Serialize)]
pub struct NewInvoiceRequest {
    invoice_id: String,
    method: String,
    amount_atomic: u64,
    token: String,
}

#[derive(serde::Deserialize)]
pub struct NewInvoiceResponse {
    pub address: String,
    pub amount: String,
    pub method: String,
}