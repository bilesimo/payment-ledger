use chrono::{DateTime, Utc};

use crate::{
    modules::{
        accounts::domain::AccountType,
        journal::domain::{BalanceSnapshot, JournalTransaction, StatementPage},
    },
    shared::{
        errors::AppError,
        ids::{AccountId, EntryId, TransactionId},
    },
};

#[derive(Debug, Clone)]
pub struct PersistedTransaction {
    pub transaction: JournalTransaction,
    pub payload_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct PostTransactionResult {
    pub transaction: JournalTransaction,
    pub was_replayed: bool,
}

#[derive(Debug, Clone)]
pub struct StatementCursor {
    pub created_at: DateTime<Utc>,
    pub transaction_id: TransactionId,
    pub entry_id: EntryId,
}

#[derive(Debug, Clone)]
pub struct StatementQuery {
    pub cursor: Option<StatementCursor>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct AccountBalanceRecord {
    pub account_type: AccountType,
    pub snapshot: BalanceSnapshot,
}

#[allow(async_fn_in_trait)]
pub trait JournalRepository: Send + Sync {
    async fn post_transaction(
        &self,
        transaction: PersistedTransaction,
    ) -> Result<PostTransactionResult, AppError>;

    async fn reverse_transaction(
        &self,
        transaction: PersistedTransaction,
    ) -> Result<PostTransactionResult, AppError>;

    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<JournalTransaction>, AppError>;

    async fn get_transaction_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<JournalTransaction>, AppError>;

    async fn get_balance(
        &self,
        account_id: AccountId,
    ) -> Result<Option<AccountBalanceRecord>, AppError>;

    async fn get_statement(
        &self,
        account_id: AccountId,
        query: &StatementQuery,
    ) -> Result<Option<StatementPage>, AppError>;
}
