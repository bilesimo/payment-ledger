use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    modules::journal::{
        domain::{
            BalanceSnapshot, EntryDirection, JournalEntry, JournalTransaction, PostTransaction,
            StatementPage, derive_net_balance, normalize_description, normalize_reference,
            validate_posting,
        },
        repository::{
            AccountBalanceRecord, JournalRepository, PersistedTransaction, PostTransactionResult,
            StatementCursor, StatementQuery,
        },
        store::PgJournalStore,
    },
    shared::{
        errors::{AppError, ErrorCode},
        ids::{AccountId, EntryId, SnowflakeGenerator, TransactionId},
    },
};

const DEFAULT_STATEMENT_LIMIT: usize = 50;
const MAX_STATEMENT_LIMIT: usize = 100;

#[derive(Clone)]
pub struct JournalService {
    repository: PgJournalStore,
    id_generator: Arc<SnowflakeGenerator>,
}

impl JournalService {
    pub fn new(repository: PgJournalStore, id_generator: Arc<SnowflakeGenerator>) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn post_transaction(
        &self,
        mut request: PostTransaction,
    ) -> Result<PostTransactionResult, AppError> {
        request.reference = normalize_reference(request.reference)?;
        request.description = normalize_description(request.description);
        validate_posting(&request.entries)?;

        let created_at = Utc::now();
        let transaction_id = self.id_generator.next_transaction_id()?;
        let mut entries = Vec::with_capacity(request.entries.len());

        for draft in request.entries {
            entries.push(JournalEntry {
                id: self.id_generator.next_entry_id()?,
                transaction_id,
                account_id: draft.account_id,
                direction: draft.direction,
                amount: draft.amount,
                created_at,
            });
        }

        let transaction = JournalTransaction {
            id: transaction_id,
            reference: request.reference,
            description: request.description,
            reversal_of_transaction_id: None,
            entries,
            created_at,
        };

        let fingerprint = fingerprint_transaction(&transaction)?;

        self.repository
            .post_transaction(PersistedTransaction {
                transaction,
                payload_fingerprint: fingerprint,
            })
            .await
    }

    pub async fn reverse_transaction(
        &self,
        original_transaction_id: TransactionId,
        reference: String,
        description: Option<String>,
    ) -> Result<PostTransactionResult, AppError> {
        let original = self
            .repository
            .get_transaction(original_transaction_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::TransactionNotFound,
                    format!("transaction {original_transaction_id} was not found"),
                )
            })?;

        if original.reversal_of_transaction_id.is_some() {
            return Err(AppError::validation(
                ErrorCode::ReversalNotAllowed,
                "reversing a reversal is not allowed in v1".to_owned(),
            ));
        }

        let created_at = Utc::now();
        let reversal_transaction_id = self.id_generator.next_transaction_id()?;

        let entries = original
            .entries
            .iter()
            .map(|entry| {
                Ok(JournalEntry {
                    id: self.id_generator.next_entry_id()?,
                    transaction_id: reversal_transaction_id,
                    account_id: entry.account_id,
                    direction: match entry.direction {
                        EntryDirection::Debit => EntryDirection::Credit,
                        EntryDirection::Credit => EntryDirection::Debit,
                    },
                    amount: entry.amount,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let reversal = JournalTransaction {
            id: reversal_transaction_id,
            reference: normalize_reference(reference)?,
            description: normalize_description(description)
                .or_else(|| Some(format!("Reversal of transaction {}", original.id))),
            reversal_of_transaction_id: Some(original.id),
            entries,
            created_at,
        };

        let fingerprint = fingerprint_transaction(&reversal)?;

        self.repository
            .reverse_transaction(PersistedTransaction {
                transaction: reversal,
                payload_fingerprint: fingerprint,
            })
            .await
    }

    pub async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<JournalTransaction, AppError> {
        self.repository
            .get_transaction(transaction_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::TransactionNotFound,
                    format!("transaction {transaction_id} was not found"),
                )
            })
    }

    pub async fn get_transaction_by_reference(
        &self,
        reference: String,
    ) -> Result<JournalTransaction, AppError> {
        let reference = normalize_reference(reference)?;

        self.repository
            .get_transaction_by_reference(&reference)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::TransactionNotFound,
                    format!("transaction reference {reference} was not found"),
                )
            })
    }

    pub async fn get_balance(&self, account_id: AccountId) -> Result<BalanceSnapshot, AppError> {
        let AccountBalanceRecord {
            account_type,
            mut snapshot,
        } = self
            .repository
            .get_balance(account_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::AccountNotFound,
                    format!("account {account_id} was not found"),
                )
            })?;

        snapshot.net_in_cents = derive_net_balance(account_type, snapshot.debits, snapshot.credits);

        Ok(snapshot)
    }

    pub async fn get_statement(
        &self,
        account_id: AccountId,
        cursor: Option<String>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<StatementPage, AppError> {
        let limit = match limit {
            Some(value) => value.clamp(1, MAX_STATEMENT_LIMIT),
            None => DEFAULT_STATEMENT_LIMIT,
        };

        let query = StatementQuery {
            cursor: cursor
                .map(|value| decode_statement_cursor(&value))
                .transpose()?,
            from,
            to,
            limit,
        };

        self.repository
            .get_statement(account_id, &query)
            .await?
            .ok_or_else(|| {
                AppError::not_found(
                    ErrorCode::AccountNotFound,
                    format!("account {account_id} was not found"),
                )
            })
    }
}

#[derive(Serialize)]
struct FingerprintEntry {
    account_id: i64,
    direction: EntryDirection,
    amount_in_cents: i64,
}

#[derive(Serialize)]
struct FingerprintPayload<'a> {
    reference: &'a str,
    description: Option<&'a str>,
    reversal_of_transaction_id: Option<i64>,
    entries: Vec<FingerprintEntry>,
}

fn fingerprint_transaction(transaction: &JournalTransaction) -> Result<String, AppError> {
    let payload = FingerprintPayload {
        reference: &transaction.reference,
        description: transaction.description.as_deref(),
        reversal_of_transaction_id: transaction
            .reversal_of_transaction_id
            .map(|value| value.value()),
        entries: transaction
            .entries
            .iter()
            .map(|entry| FingerprintEntry {
                account_id: entry.account_id.value(),
                direction: entry.direction,
                amount_in_cents: entry.amount.amount_in_cents(),
            })
            .collect(),
    };

    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("failed to serialize idempotency payload: {error}"),
        )
    })?;

    Ok(format!("{:x}", Sha256::digest(encoded)))
}

#[derive(Serialize, Deserialize)]
struct CursorPayload {
    created_at: DateTime<Utc>,
    transaction_id: i64,
    entry_id: i64,
}

pub fn encode_statement_cursor(cursor: &StatementCursor) -> Result<String, AppError> {
    let payload = CursorPayload {
        created_at: cursor.created_at,
        transaction_id: cursor.transaction_id.value(),
        entry_id: cursor.entry_id.value(),
    };
    let json = serde_json::to_vec(&payload).map_err(|error| {
        AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("failed to serialize statement cursor: {error}"),
        )
    })?;

    Ok(URL_SAFE_NO_PAD.encode(json))
}

fn decode_statement_cursor(value: &str) -> Result<StatementCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|error| {
        AppError::validation(
            ErrorCode::InvalidRequest,
            format!("statement cursor is invalid: {error}"),
        )
    })?;

    let payload: CursorPayload = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::validation(
            ErrorCode::InvalidRequest,
            format!("statement cursor is invalid: {error}"),
        )
    })?;

    Ok(StatementCursor {
        created_at: payload.created_at,
        transaction_id: payload.transaction_id.into(),
        entry_id: EntryId::new(payload.entry_id),
    })
}
