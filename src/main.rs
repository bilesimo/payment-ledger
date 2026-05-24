use dotenv::dotenv;
use payment_ledger::{
    build_app,
    infra::config::Config,
    shared::errors::{AppError, ErrorCode},
};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let app = build_app(&config).await?;
    let listener = TcpListener::bind(config.http_addr).await.map_err(|error| {
        AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("failed to bind listener on {}: {error}", config.http_addr),
        )
    })?;

    info!(address = %config.http_addr, "payment-ledger listening");

    axum::serve(listener, app).await.map_err(|error| {
        AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("server terminated unexpectedly: {error}"),
        )
    })
}
