use std::sync::Arc;

use chrono::Utc;

use crate::{
    modules::accounts::{
        domain::{Account, CreateAccount},
        repository::AccountRepository,
        store::PgAccountStore,
    },
    shared::{
        errors::{AppError, ErrorCode},
        ids::{AccountId, SnowflakeGenerator},
    },
};

#[derive(Clone)]
pub struct AccountService {
    repository: PgAccountStore,
    id_generator: Arc<SnowflakeGenerator>,
}

impl AccountService {
    pub fn new(repository: PgAccountStore, id_generator: Arc<SnowflakeGenerator>) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn create_account(
        &self,
        mut create_account: CreateAccount,
    ) -> Result<Account, AppError> {
        create_account.name = normalize_name(create_account.name);

        let account = Account::new(
            self.id_generator.next_account_id()?,
            create_account.name,
            create_account.account_type,
            Utc::now(),
        );

        self.repository.create_account(&account).await?;

        Ok(account)
    }

    pub async fn get_account(&self, account_id: AccountId) -> Result<Account, AppError> {
        self.repository
            .get_account(account_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::AccountNotFound,
                    format!("account {account_id} was not found"),
                )
            })
    }
}

fn normalize_name(name: Option<String>) -> Option<String> {
    name.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}
