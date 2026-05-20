use payment_system_structs::CallbackHeaders;
use sqlx::PgConnection;

pub mod macros_400;
pub mod poll_payments;

async fn fetch_callback_headers(
    conn: &mut PgConnection,
    invoice_id: &str,
) -> Result<CallbackHeaders, String> {
    let Ok(rows) = sqlx::query!(
        "SELECT key, value FROM callback_headers WHERE invoice_id = $1",
        invoice_id
    )
    .fetch_all(conn)
    .await
    else {
        return Err(format!("DB err"));
    };

    let map = rows.into_iter().map(|r| (r.key, r.value)).collect();
    Ok(CallbackHeaders(map))
}