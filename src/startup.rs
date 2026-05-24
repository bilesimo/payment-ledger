use std::sync::Arc;

use axum::Router;

use crate::{
    infra::{config::Config, db::connect_pool},
    modules::{
        accounts::{http as accounts_http, service::AccountService, store::PgAccountStore},
        journal::{http as journal_http, service::JournalService, store::PgJournalStore},
    },
    shared::{
        errors::{AppError, ErrorCode},
        ids::SnowflakeGenerator,
    },
    ui,
};

#[derive(Clone)]
pub struct AppState {
    pub account_service: Arc<AccountService>,
    pub journal_service: Arc<JournalService>,
}

pub async fn build_app(config: &Config) -> Result<Router, AppError> {
    let pool = connect_pool(&config.database_url).await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| {
            AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("failed to run migrations: {error}"),
            )
        })?;

    let id_generator = Arc::new(SnowflakeGenerator::new(config.node_id)?);
    let account_store = PgAccountStore::new(pool.clone());
    let journal_store = PgJournalStore::new(pool);

    let state = AppState {
        account_service: Arc::new(AccountService::new(account_store, id_generator.clone())),
        journal_service: Arc::new(JournalService::new(journal_store, id_generator)),
    };

    Ok(Router::new()
        .merge(ui::router())
        .merge(accounts_http::router())
        .merge(journal_http::router())
        .with_state(state))
}
