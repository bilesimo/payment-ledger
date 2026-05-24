use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::shared::errors::{AppError, ErrorCode};

pub async fn connect_pool(database_url: &str) -> Result<PgPool, AppError> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .map_err(|error| {
            AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("failed to connect to postgres: {error}"),
            )
        })
}
