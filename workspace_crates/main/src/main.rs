use std::path::PathBuf;

use server::http_stuff::http_response::ok_html;


#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    let service_config = server::config_reader::read_config::<PaymentSystemConfig>(PathBuf::from(".env"));
    let db_pool = sqlx::PgPool::connect(&service_config.database_url)
        .await
        .expect("I expected pool construction to succeed :)");
    let handler: server::run::HandlerFn =
        std::sync::Arc::new(move |_request_data, _server_config| {
            let _db_pool = db_pool.clone();
            let _service_config = service_config.clone();
            Box::pin(async move {
                ok_html("<b>Hello world</b>".to_string())
            })
        });
    server::run(handler, app_executable_version::get()).await
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PaymentSystemConfig {
    pub database_url: String,
}