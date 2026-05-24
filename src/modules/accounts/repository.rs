use crate::{
    modules::accounts::domain::Account,
    shared::{errors::AppError, ids::AccountId},
};

#[allow(async_fn_in_trait)]
pub trait AccountRepository: Send + Sync {
    async fn create_account(&self, account: &Account) -> Result<(), AppError>;
    async fn get_account(&self, account_id: AccountId) -> Result<Option<Account>, AppError>;
}
