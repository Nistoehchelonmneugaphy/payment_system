use main::poll_payments::poll_payments;
use main::{ok_or_400, ok_or_err_400, some_or_400};
use server::http_stuff::http_response::ContentType;
use std::{path::PathBuf, str::FromStr};
use tokio_util::sync::CancellationToken;
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
                let (prefix, method_str) = some_or_400!(
                    parse_path(&request_data.path),
                    format!("Could not parse {}", &request_data.path)
                );
                if prefix != payment_system_config.secret_prefix {
                    println!("prefix {prefix} != {}", payment_system_config.secret_prefix);
                    return server::http_stuff::http_response::bad_request_400();
                }
                let token = some_or_400!(
                    request_data.headers.get("x-api-token"),
                    "x-api-token header not found in request"
                )
                .to_string();
                println!("User used token {token}");
                ok_or_400!(
                    validate_token(db_pool.clone(), token.clone()).await,
                    format!("Could not validate token {token}")
                );
                let method = ok_or_400!(
                    APIMethod::from_str(method_str),
                    "Could not parse method".to_string()
                );
                println!("Method: {method:#?}");
                match method {
                    APIMethod::NewInvoice => {
                        let mut tx = ok_or_400!(db_pool.begin().await, String::new());
                        ok_or_400!(
                            deny_if_too_many_active_invoices(
                                &mut tx,
                                payment_system_config.clone()
                            )
                            .await,
                            format!("Too mamny invoices active")
                        );
                        let invoice_req = ok_or_400!(
                            serde_json::from_slice::<NewInvoiceRequest>(&request_data.body_bytes,),
                            format!(
                                "Could not parse json from {}",
                                String::from_utf8_lossy(&request_data.body_bytes)
                            )
                        );
                        println!("Invoice: {invoice_req:#?}");
                        let gened_address = ok_or_400!(
                            dynamic_address_generation(
                                &mut *tx,
                                &payment_system_config.clone(),
                                &invoice_req.method.clone()
                            ).await, format!("Could not generate address for {}", invoice_req.method)
                        );
                        let address = match gened_address {
                            GeneratedAddress::Litecoin { index:_, address } => address,
                            GeneratedAddress::Monero { payment_id: _, address } => address,
                        };

                        let id = ok_or_400!(sqlx::query!("INSERT INTO payments (status_update_callback, success_callback, method, address, amount) VALUES($1, $2, $3, $4, $5) RETURNING id", invoice_req.status_update_callback,  invoice_req.success_callback,  invoice_req.method.to_string(), address.to_string(), invoice_req.amount_atomic.to_string()).fetch_one(&mut *tx).await, "sqlx error").id;
                        ok_or_400!(tx.commit().await);
                        let payment_info = PaymentInfo {
                            payment_id: PaymentId(id),
                            amount: Amount(invoice_req.amount_atomic),
                            address,
                            method: invoice_req.method,
                        };
                        println!("{payment_info:#?}");
                        let content = ok_or_400!(serde_json::to_vec(&payment_info));
                        return ok(content, ContentType::ApplicationJson, Some(Charset::Utf8));
                    }
                };
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
            cancellation_token: Some(cancellation_token),
        },
    )
    .await;
    Ok(())
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

#[derive(Debug, strum::Display)]
pub enum TokenValidationError {
    NoTokenInHeaders,
    DbError,
    TokenNotFound,
}
async fn validate_token(db_pool: PgPool, token_str: String) -> Result<(), TokenValidationError> {
    let _token = {
        let exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tokens WHERE token = $1)")
                .bind(&token_str)
                .fetch_one(&db_pool)
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
    Ok(())
}

use cryptocurrencies_stuff::{
    derive_ltc::derive_ltc_bip84_address,
};
use payment_system_structs::{
    Address, AddressIndex, Amount, NewInvoiceRequest, PaymentId, PaymentInfo, PaymentMethod, PaymentSystemConfig
};
use server::http_stuff::http_response::{Charset, HttpResponseData, bad_request_400, ok};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use strum::EnumString;

#[derive(EnumString, Debug)]
#[strum(serialize_all = "snake_case")]
pub enum APIMethod {
    NewInvoice,
}

pub enum GeneratedAddress {
    Litecoin {
        index: AddressIndex,
        address: Address,
    },
    Monero {
        payment_id: PaymentId,
        address: Address,
    }
}

async fn dynamic_address_generation(
    conn: &mut PgConnection,
    psc: &PaymentSystemConfig,
    m: &PaymentMethod,
) -> Result<GeneratedAddress, String> {
    let Ok(Some(index)) = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM payments WHERE method = $1",
        m.to_string()
    )
    .fetch_one(conn)
    .await
    else {
        println!("Could not get count {}", m.to_string());
        return Err(format!("..."));
    };

    let index = index as u64;
    match m {
        PaymentMethod::Litecoin => {
            let Ok(address) = derive_ltc_bip84_address(&psc.master_seed, index) else {
                println!("Litecoin addr error");
                return Err(format!("Couldn't derive litecoin"));
            };
            Ok(GeneratedAddress::Litecoin { index: AddressIndex(index), address: Address(address) })
        }
        PaymentMethod::Monero => {
            let (address, payment_id) = match get_monero_address_and_payment_id(&psc.monero_wallet_rpc_url).await {
                Ok(a) => a,
                Err(e) => {
                    return Err(format!("Err '{e}' from monero subaddr gen"));
                }
            };
            Ok(GeneratedAddress::Monero { payment_id, address, })
        }
    }
}

fn parse_path(path: &str) -> Option<(&str, &str)> {
    // strips leading '/' then splits on the next '/'
    let path = path.strip_prefix('/')?;
    let (prefix, method) = path.split_once('/')?;
    Some((prefix, method))
}



pub async fn get_monero_address_and_payment_id(rpc_url: &str) -> Result<(Address, PaymentId,), String> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({"jsonrpc": "2.0",
    "id": "0",
    "method": "make_integrated_address"
  });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Monero RPC request failed: {e}"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Monero RPC parse failed: {e}"))?;

    let address = resp["result"]["integrated_address"]
        .as_str()
        .ok_or_else(|| format!("Unexpected RPC response: {resp}"))?
        .to_string();
    let payment_id = resp["result"]["payment_id"]
        .as_str()
        .ok_or_else(|| format!("Unexpected RPC response: {resp}"))?
        .to_string();

    Ok((Address(address), PaymentId(payment_id)))
}