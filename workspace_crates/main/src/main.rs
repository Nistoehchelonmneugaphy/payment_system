use std::{path::PathBuf, str::FromStr, time::Duration};
use tokio_util::sync::CancellationToken;
macro_rules! ok_or_400 {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(_) => return server::http_stuff::http_response::bad_request_400(),
        }
    };
}
macro_rules! ok_or_err_400 {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(_) => return Err(server::http_stuff::http_response::bad_request_400()),
        }
    };
}
macro_rules! some_or_400 {
    ($expr:expr) => {
        match $expr {
            Some(v) => v,
            None => return server::http_stuff::http_response::bad_request_400(),
        }
    };
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cancellation_token = CancellationToken::new();
    let poll_payments_token = cancellation_token.child_token();
    dotenvy::dotenv().ok();
    let payment_system_config =
        server::config_reader::read_config::<PaymentSystemConfig>(PathBuf::from(".env"));
    let db_pool = sqlx::PgPool::connect(&payment_system_config.database_url)
        .await
        .expect("I expected pool construction to succeed :)");
    let db_pool_clone_for_handler = db_pool.clone();
    let payment_system_config_clone_for_handler = payment_system_config.clone();

    let handler: server::run::HandlerFn = std::sync::Arc::new(
        move |request_data, _server_config| {
            let db_pool = db_pool_clone_for_handler.clone();
            let payment_system_config = payment_system_config_clone_for_handler.clone();
            Box::pin(async move {
                let (prefix, method_str) = some_or_400!(parse_path(&request_data.path));
                if prefix != payment_system_config.secret_prefix {
                    return server::http_stuff::http_response::bad_request_400();
                }
                let mut tx = ok_or_400!(db_pool.begin().await);
                let _token =
                    ok_or_400!(validate_token(&mut tx, request_data.headers.clone()).await);
                let method = ok_or_400!(APIMethod::from_str(method_str));
                println!("Method: {method:#?}");
                match method {
                    APIMethod::NewInvoice => {
                        ok_or_400!(
                            deny_if_too_many_active_invoices(
                                &mut tx,
                                payment_system_config.clone()
                            )
                            .await
                        );
                        let req = ok_or_400!(serde_json::from_slice::<NewInvoiceRequest>(
                            &request_data.body_bytes,
                        ));
                        let (index, address) = ok_or_400!(
                            dynamic_address_generation(
                                &mut *tx,
                                &payment_system_config.clone(),
                                &req.method.clone()
                            )
                            .await
                        );

                        ok_or_400!(sqlx::query!("INSERT INTO invoice_payment_data (invoice_id, status_update_callback, success_callback, method, generated_address, address_index, amount) VALUES($1, $2, $3, $4, $5, $6, $7)", req.invoice_id, req.status_update_callback,  req.success_callback,  req.method.to_string(), address.to_string(), index.to_string(), req.amount_atomic.to_string()).execute(&mut *tx).await);
                    }
                }
                ok_or_400!(tx.commit().await);
                todo!();
            })
        },
    );

    let poll_payments_task = Box::pin(poll_payments(
        db_pool.clone(),
        payment_system_config.clone(),
        poll_payments_token,
    ));
    let _ = server::run(
        app_executable_version::get(),
        handler,
        server::run::BackgroundSpawnTasks {
            new_tasks_to_spawn: Some(vec![poll_payments_task]),
        },
    )
    .await;
    Ok(())
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

async fn poll_payments(
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
async fn deny_if_too_many_active_invoices(
    tx: &mut Transaction<'_, Postgres>,
    payment_system_config: PaymentSystemConfig,
) -> Result<(), HttpResponseData> {
    let interval = format!(
        "{} minutes",
        payment_system_config.active_invoice_minutes_since_creation
    );

    let count: i64 = ok_or_err_400!(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoice_payment_data WHERE created_at > NOW() - $1::interval"
        )
        .bind(&interval)
        .fetch_one(&mut **tx)
        .await
    );
    let count = ok_or_err_400!(u64::try_from(count));
    if count > payment_system_config.max_active_invoices {
        return Err(bad_request_400());
    }
    Ok(())
}

pub enum TokenValidationError {
    NoTokenInHeaders,
    DbError,
    TokenNotFound,
}
async fn validate_token(
    tx: &mut Transaction<'_, Postgres>,
    headers: Headers,
) -> Result<String, TokenValidationError> {
    let token = {
        let Some(token_str) = headers.get("x-api-token") else {
            return Err(TokenValidationError::NoTokenInHeaders);
        };
        let exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tokens WHERE token = $1)")
                .bind(&token_str)
                .fetch_one(&mut **tx)
                .await
            {
                Ok(b) => b,
                Err(_) => {
                    return Err(TokenValidationError::DbError);
                }
            };
        if !exists {
            return Err(TokenValidationError::TokenNotFound);
        }
        token_str.to_string()
    };
    Ok(token)
}

use cryptocurrencies_stuff::{
    derive_ltc::derive_ltc_bip84_address, monero_wallet_rpc::monero_create_subaddress,
};
use payment_system_structs::{
    Address, AddressIndex, CallbackHeaders, NewInvoiceRequest, PaymentMethod,
};
use server::http_stuff::{
    http_request_parsing::Headers,
    http_response::{HttpResponseData, bad_request_400},
};
use sqlx::{PgConnection, Postgres, Transaction, postgres::types::PgInterval};
use strum::EnumString;

#[derive(EnumString, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum APIMethod {
    NewInvoice,
}

async fn dynamic_address_generation(
    conn: &mut PgConnection,
    psc: &PaymentSystemConfig,
    m: &PaymentMethod,
) -> Result<(AddressIndex, Address), String> {
    let Ok(Some(index)) = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM invoice_payment_data WHERE method = $1",
        m.to_string()
    )
    .fetch_one(conn)
    .await
    else {
        return Err(format!("..."));
    };

    let index = index as u64;
    let address = match m {
        PaymentMethod::Litecoin => {
            let Ok(address) = derive_ltc_bip84_address(&psc.master_seed, index) else {
                return Err(format!("Couldn't derive litecoin"));
            };
            address
        }
        PaymentMethod::Monero => {
            let Ok(addr) = monero_create_subaddress(&psc.monero_wallet_rpc_url).await else {
                return Err(format!("could not derive subaddress monero"));
            };
            addr
        }
    };
    Ok((AddressIndex(index), Address(address)))
}

fn parse_path(path: &str) -> Option<(&str, &str)> {
    // strips leading '/' then splits on the next '/'
    let path = path.strip_prefix('/')?;
    let (prefix, method) = path.split_once('/')?;
    Some((prefix, method))
}

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
