use sqlx::{PgPool, Row};

use crate::{
    modules::accounts::{
        domain::{Account, AccountType},
        repository::AccountRepository,
    },
    shared::{
        errors::{AppError, ErrorCode},
        ids::AccountId,
        money::Currency,
    },
};

#[derive(Clone)]
pub struct PgAccountStore {
    pool: PgPool,
}

impl PgAccountStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl AccountRepository for PgAccountStore {
    async fn create_account(&self, account: &Account) -> Result<(), AppError> {
        sqlx::query(
            r#"
            insert into accounts (id, name, account_type, currency, created_at)
            values ($1, $2, $3::account_type, $4, $5)
            "#,
        )
        .bind(account.id.value())
        .bind(account.name.as_deref())
        .bind(account.account_type.as_db_value())
        .bind(account.currency.as_str())
        .bind(account.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("failed to insert account: {error}"),
            )
        })?;

        Ok(())
    }

    async fn get_account(&self, account_id: AccountId) -> Result<Option<Account>, AppError> {
        let row = sqlx::query(
            r#"
            select id, name, account_type::text as account_type, currency, created_at
            from accounts
            where id = $1
            "#,
        )
        .bind(account_id.value())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("failed to fetch account: {error}"),
            )
        })?;

        row.map(map_account).transpose()
    }
}

fn map_account(row: sqlx::postgres::PgRow) -> Result<Account, AppError> {
    let account_type = AccountType::from_db_value(&row.get::<String, _>("account_type"))?;

    let currency: String = row.get("currency");
    if currency != Currency::Brl.as_str() {
        return Err(AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("unknown currency stored in database: {currency}"),
        ));
    }

    Ok(Account::new(
        AccountId::new(row.get("id")),
        row.get("name"),
        account_type,
        row.get("created_at"),
    ))
}
