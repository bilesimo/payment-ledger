use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    modules::journal::domain::EntryDirection,
    shared::ids::{AccountId, EntryId, TransactionId},
};

#[derive(Debug, Deserialize)]
pub struct CreateTransactionRequest {
    pub reference: String,
    pub description: Option<String>,
    pub entries: Vec<CreateTransactionEntryRequest>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTransactionEntryRequest {
    pub account_id: AccountId,
    pub direction: EntryDirection,
    pub amount_in_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct ReverseTransactionRequest {
    pub reference: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JournalEntryResponse {
    pub entry_id: EntryId,
    pub account_id: AccountId,
    pub direction: EntryDirection,
    pub amount_in_cents: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct JournalTransactionResponse {
    pub transaction_id: TransactionId,
    pub reference: String,
    pub description: Option<String>,
    pub reversal_of_transaction_id: Option<TransactionId>,
    pub created_at: DateTime<Utc>,
    pub entries: Vec<JournalEntryResponse>,
}

#[derive(Debug, Serialize)]
pub struct PostTransactionResponse {
    pub transaction: JournalTransactionResponse,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub account_id: AccountId,
    pub currency: &'static str,
    pub debits_in_cents: i64,
    pub credits_in_cents: i64,
    pub net_in_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct StatementEntryResponse {
    pub entry_id: EntryId,
    pub transaction_id: TransactionId,
    pub reference: String,
    pub description: Option<String>,
    pub direction: EntryDirection,
    pub amount_in_cents: i64,
    pub running_balance_in_cents: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct StatementResponse {
    pub entries: Vec<StatementEntryResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatementQueryParams {
    pub cursor: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}
