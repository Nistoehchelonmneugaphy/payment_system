use std::{path::PathBuf, str::FromStr};

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
    dotenvy::dotenv().ok();
    let payment_system_config =
        server::config_reader::read_config::<PaymentSystemConfig>(PathBuf::from(".env"));
    let db_pool = sqlx::PgPool::connect(&payment_system_config.database_url)
        .await
        .expect("I expected pool construction to succeed :)");
    let handler: server::run::HandlerFn = std::sync::Arc::new(
        move |request_data, _server_config| {
            let db_pool = db_pool.clone();
            let payment_system_config = payment_system_config.clone();
            Box::pin(async move {
                let (prefix, method_str) = some_or_400!(parse_path(&request_data.path));
                if prefix != payment_system_config.secret_prefix {
                    return server::http_stuff::http_response::bad_request_400();
                }
                let mut tx = ok_or_400!(db_pool.begin().await);
                let _token =
                    ok_or_400!(validate_token(&mut tx, request_data.headers.clone()).await);
                let method = ok_or_400!(APIMethod::from_str(method_str));

                match method {
                    APIMethod::NewInvoice => {
                        ok_or_400!(deny_if_too_many_active_invoices(&mut tx, payment_system_config.clone()).await);
                        let req = ok_or_400!(serde_json::from_slice::<NewInvoiceRequest>(
                            &request_data.body_bytes,
                        ));
                        let address = ok_or_400!(dynamic_address_generation(req.method.clone()));

                        ok_or_400!(sqlx::query!("INSERT INTO invoice_payment_data (invoice_id, status_update_callback, success_callback, method, generated_address, amount) VALUES($1, $2, $3, $4, $5, $6)", req.invoice_id, req.status_update_callback,  req.success_callback,  req.method.to_string(), address, req.amount_atomic.to_string()).execute(&mut *tx).await);
                    }
                }
                ok_or_400!(tx.commit().await);
                todo!();
            })
        },
    );
    let _ = server::run(handler, app_executable_version::get()).await;
    Ok(())
}

async fn deny_if_too_many_active_invoices(
    tx: &mut Transaction<'_, Postgres>,
    payment_system_config: PaymentSystemConfig,
) -> Result<(), HttpResponseData> {
    let count: i64 = ok_or_err_400!(sqlx::query_scalar(
    "SELECT COUNT(*) FROM invoice_payment_data WHERE created_at > NOW() - INTERVAL '55 minutes'"
)
.fetch_one(&mut **tx)
.await);
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

use payment_system_structs::{NewInvoiceRequest, PaymentMethod};
use server::http_stuff::{
    http_request_parsing::Headers,
    http_response::{HttpResponseData, bad_request_400},
};
use sqlx::{Postgres, Transaction};
use strum::EnumString;

#[derive(EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum APIMethod {
    NewInvoice,
}
fn dynamic_address_generation(m: PaymentMethod) -> Result<String, String> {
    todo!();
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
    // limit monitoring load
    pub max_active_invoices: u64,
}
