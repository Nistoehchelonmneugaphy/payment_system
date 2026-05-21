use std::time::Duration;

use payment_system_structs::{Payment, PaymentId, PaymentSystemConfig};
use sqlx::postgres::types::PgInterval;
use tokio_util::sync::CancellationToken;

use crate::fetch_callback_headers;

pub async fn poll_payments(
    db_pool: sqlx::PgPool,
    payment_system_config: PaymentSystemConfig,
    cancellation_token: CancellationToken,
    sleep_between_each_payment_check: u64,
    sleep_between_each_loop: u64,
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
        let payments = sqlx::query_as!(
            Payment,
            "SELECT * FROM payments  WHERE completed_at IS NULL
       AND created_at > NOW() - $1::interval",
            &interval
        )
        .fetch_all(&db_pool)
        .await
        .unwrap_or_default();
        println!("Polling for {} payments", payments.len());
        for payment in payments {
            match payment.method.as_str() {
                "monero" => {
                    let Ok(paid) = check_monero_payment(
                        db_pool.clone(),
                        &payment,
                        &rpc,
                        &payment_system_config,
                    )
                    .await
                    else {
                        continue;
                    };
                    let Ok(mut tx) = db_pool.begin().await else {
                        continue;
                    };
                    if paid {
                        let result = sqlx::query!(
                            "UPDATE payments 
                             SET completed_at = NOW() 
                             WHERE id = $1 
                             AND completed_at IS NULL", // guard against double update
                            &payment.id
                        )
                        .execute(&mut *tx)
                        .await;
                        if result.is_err() {
                            println!("Failed to mark invoice {} as complete", payment.id);
                            continue;
                        }
                        let Ok(headers) =
                            fetch_callback_headers(&mut *tx, PaymentId(payment.id.clone())).await
                        else {
                            continue;
                        };

                        let mut request = rpc.post(&payment.success_callback);
                        println!("Sending success_callback, payment_id: {}", payment.id);
                        request = request.body(format!("payment_id={}", payment.id));
                        for (key, value) in &headers.0 {
                            request = request.header(key, value);
                        }

                        let resp =match request.send().await {
                            Ok(resp) => resp,
                            Err(_) => continue,
                        };

                        println!("Received resp after successful payment request: {}", resp.text().await.unwrap_or("None".to_string()));
                        let _ = tx.commit().await;
                    }
                }
                "litecoin" => todo!(),
                _ => continue,
            }
            if cancellable_sleep(
                Duration::from_secs(sleep_between_each_payment_check),
                &cancellation_token,
            )
            .await
            .is_err()
            {
                tracing::info!("Poll payments task cancelled during invoice delay");
                return;
            }
        }
        if cancellable_sleep(
            Duration::from_secs(sleep_between_each_loop),
            &cancellation_token,
        )
        .await
        .is_err()
        {
            tracing::info!("Poll payments task cancelled during main sleep");
            return;
        }
    }
}

async fn check_monero_payment(
    db_pool: sqlx::PgPool,
    payment: &Payment,
    client: &reqwest::Client,
    payment_system_config: &PaymentSystemConfig,
) -> anyhow::Result<bool> {
    let monero_payment_id = sqlx::query!(
        "SELECT monero_payment_id FROM monero_integrated_payment_ids WHERE payment_id = $1",
        payment.id
    )
    .fetch_one(&db_pool)
    .await?
    .monero_payment_id;
    let expected_amount: u64 = payment.amount.parse()?; // already in piconer
    println!(
        "[Payment ID {}] Checking if the expect amount of {expected_amount} piconero was paid to {}",
        payment.id, payment.address
    );
    let body = serde_json::json!({
      "jsonrpc": "2.0",
      "id": "0",
      "method": "get_payments",
      "params": {
        "payment_id": monero_payment_id
      }
    });
    let res: serde_json::Value = client
        .post(&payment_system_config.monero_wallet_rpc_url)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    let transfers = res["result"]["payments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let current_height_body = serde_json::json!({"jsonrpc":"2.0","id":"0","method":"get_height"});
    let res_height: serde_json::Value = client
        .post(&payment_system_config.monero_wallet_rpc_url)
        .json(&current_height_body)
        .send()
        .await?
        .json()
        .await?;
    let current_height = res_height["result"]["block_height"].as_u64().unwrap_or(0);
    println!("Curr height: {current_height}");
    for transfer in transfers.clone() {
        let height = transfer["height"].as_u64().unwrap_or(999999999);
        if current_height < height {
            println!("A transfer: to unlock: height: {}", height - current_height);
        } else {
            println!("An unlock tx");
        }
    }
    if transfers.len() > 0 {
        println!("Received some transfers!! {transfers:#?}");
    }
    let confirmed_received: u64 = transfers
        .iter()
        .filter(|t| t["locked"].as_bool().unwrap_or(true) != true)
        .map(|t| t["amount"].as_u64().unwrap_or(0))
        .sum();
    let total_received: u64 = transfers
        .iter()
        .map(|t| t["amount"].as_u64().unwrap_or(0))
        .sum();
    println!("Confirmed: {confirmed_received}");
    println!("Confirmed + unconfirmed: {total_received}");
    let complete = confirmed_received >= expected_amount;

    match complete {
        false => println!("not enough"),
        true => println!("yay, enough"),
    }
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
