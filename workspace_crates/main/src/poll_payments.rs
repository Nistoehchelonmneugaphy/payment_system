use std::time::Duration;

use payment_system_structs::{InvoicePaymentData, PaymentSystemConfig};
use sqlx::postgres::types::PgInterval;
use tokio_util::sync::CancellationToken;

use crate::fetch_callback_headers;

pub async fn poll_payments(
    db_pool: sqlx::PgPool,
    payment_system_config: PaymentSystemConfig,
    cancellation_token: CancellationToken,
) {
    let rpc = reqwest::Client::new();
    let interval = PgInterval {
        months: 0,
        days: 0,
        microseconds: (payment_system_config.active_invoice_minutes_since_creation as i64)
            * 60
            * 1_000_000,
    };

    loop {
        let db_pool = db_pool.clone();
        let invoices = sqlx::query_as!(
            InvoicePaymentData,
            "SELECT * FROM invoice_payment_data  WHERE completed_at IS NULL
       AND created_at > NOW() - $1::interval",
            &interval
        )
        .fetch_all(&db_pool)
        .await
        .unwrap_or_default();
        for invoice in invoices {
            match invoice.method.as_str() {
                "monero" => {
                    let Ok(paid) =
                        check_monero_payment(&invoice, &rpc, &payment_system_config).await
                    else {
                        continue;
                    };
                    let Ok(mut tx) = db_pool.begin().await else {
                        continue;
                    };
                    if paid {
                        let result = sqlx::query!(
                            "UPDATE invoice_payment_data 
                             SET completed_at = NOW() 
                             WHERE invoice_id = $1 
                             AND completed_at IS NULL", // guard against double update
                            &invoice.invoice_id
                        )
                        .execute(&mut *tx)
                        .await;
                        if result.is_err() {
                            tracing::error!(
                                "Failed to mark invoice {} as complete",
                                invoice.invoice_id
                            );
                            continue;
                        }
                        let Ok(headers) =
                            fetch_callback_headers(&mut *tx, &invoice.invoice_id).await
                        else {
                            continue;
                        };

                        let mut request = rpc.post(&invoice.success_callback);

                        for (key, value) in &headers.0 {
                            request = request.header(key, value);
                        }

                        let _ = request.send().await;
                        let _ = tx.commit().await;
                    }
                }
                _ => continue,
            }
            if cancellable_sleep(Duration::from_secs(30), &cancellation_token)
                .await
                .is_err()
            {
                tracing::info!("Poll payments task cancelled during invoice delay");
                return;
            } 
        }
        if cancellable_sleep(Duration::from_mins(5), &cancellation_token)
            .await
            .is_err()
        {
            tracing::info!("Poll payments task cancelled during main sleep");
            return;
        }
    }
}

async fn check_monero_payment(
    invoice_payment_data: &InvoicePaymentData,
    client: &reqwest::Client,
    payment_system_config: &PaymentSystemConfig,
) -> anyhow::Result<bool> {
    let address_index: u32 = invoice_payment_data.address_index.parse()?;
    let expected_amount: u64 = invoice_payment_data.amount.parse()?; // already in piconer
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": "get_transfers",
        "params": {
            "in": true,
            "account_index": 0,
            "subaddr_indices": [address_index]
        }
    });
    let res: serde_json::Value = client
        .post(&payment_system_config.monero_wallet_rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    let transfers = res["result"]["in"].as_array().cloned().unwrap_or_default();
    let confirmed_received: u64 = transfers
        .iter()
        .filter(|t| {
            t["confirmations"].as_u64().unwrap_or(0) >= payment_system_config.monero_confirmations
        })
        .map(|t| t["amount"].as_u64().unwrap_or(0))
        .sum();

    Ok(confirmed_received >= expected_amount)
}

async fn cancellable_sleep(
    duration: Duration,
    cancellation_token: &CancellationToken,
) -> Result<(), ()> {
    tokio::select! {
        _ = tokio::time::sleep(duration) => Ok(()),
        _ = cancellation_token.cancelled() => Err(()),
    }
}