use std::{path::PathBuf, str::FromStr};

macro_rules! or_400 {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(_) => return server::http_stuff::http_response::bad_request_400(),
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
                let Some((prefix, method_str)) = parse_path("/secretprefix/new_payment") else {
                    return server::http_stuff::http_response::bad_request_400();
                };
                if prefix != payment_system_config.secret_prefix {
                    return server::http_stuff::http_response::bad_request_400();
                }
                let method = or_400!(APIMethod::from_str(method_str));
                match method {
                    APIMethod::NewPayment => {
                        let req = or_400!(serde_json::from_slice::<NewInvoiceRequest>(
                            &request_data.body_bytes,
                        ));
                        let address = or_400!(dynamic_address_generation(req.method.clone()));
                        let mut tx = or_400!(db_pool.begin().await);
                        or_400!(sqlx::query!("INSERT INTO invoice_payment_data (invoice_id, status_update_callback, success_callback, method, generated_address, amount) VALUES($1, $2, $3, $4, $5, $6)", req.invoice_id, req.status_update_callback,  req.success_callback,  req.method.to_string(), address, req.amount_atomic.to_string()).execute(&mut *tx).await);
                        or_400!(tx.commit().await);
                    }
                }
                todo!();
            })
        },
    );
    let _ = server::run(handler, app_executable_version::get()).await;
    Ok(())
}

use payment_system_structs::{NewInvoiceRequest, PaymentMethod};
use strum::EnumString;

#[derive(EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum APIMethod {
    NewPayment,
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
}
