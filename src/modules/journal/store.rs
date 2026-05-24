use std::collections::BTreeSet;

use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::{error, info};

use crate::{
    modules::{
        accounts::domain::AccountType,
        journal::{
            domain::{
                BalanceSnapshot, EntryDirection, JournalEntry, JournalTransaction, StatementEntry,
                StatementPage, derive_net_balance,
            },
            repository::{
                AccountBalanceRecord, JournalRepository, PersistedTransaction,
                PostTransactionResult, StatementCursor, StatementQuery,
            },
            service::encode_statement_cursor,
        },
    },
    shared::{
        errors::{AppError, ErrorCode},
        ids::{AccountId, EntryId, TransactionId},
        money::{Currency, Money},
    },
};

#[derive(Clone)]
pub struct PgJournalStore {
    pool: PgPool,
}

impl PgJournalStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl JournalRepository for PgJournalStore {
    async fn post_transaction(
        &self,
        persisted: PersistedTransaction,
    ) -> Result<PostTransactionResult, AppError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(db_error("begin transaction"))?;

        if let Some(existing) =
            find_existing_by_reference(&mut transaction, &persisted.transaction.reference).await?
        {
            if existing.payload_fingerprint == persisted.payload_fingerprint {
                let journal_transaction = load_transaction_by_id(&mut transaction, existing.id)
                    .await?
                    .ok_or_else(|| {
                        AppError::unexpected(
                            ErrorCode::Infrastructure,
                            "existing idempotent transaction disappeared during retry".to_owned(),
                        )
                    })?;
                transaction
                    .commit()
                    .await
                    .map_err(db_error("commit replayed transaction"))?;

                return Ok(PostTransactionResult {
                    transaction: journal_transaction,
                    was_replayed: true,
                });
            }

            error!(
                reference = persisted.transaction.reference,
                "idempotency conflict detected"
            );

            return Err(AppError::conflict(
                ErrorCode::IdempotencyConflict,
                "reference already exists with a different payload".to_owned(),
            ));
        }

        ensure_accounts_exist(&mut transaction, &persisted.transaction.entries).await?;
        insert_transaction(&mut transaction, &persisted).await?;
        insert_entries(&mut transaction, &persisted.transaction.entries).await?;

        transaction
            .commit()
            .await
            .map_err(db_error("commit post transaction"))?;

        Ok(PostTransactionResult {
            transaction: persisted.transaction,
            was_replayed: false,
        })
    }

    async fn reverse_transaction(
        &self,
        persisted: PersistedTransaction,
    ) -> Result<PostTransactionResult, AppError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(db_error("begin reversal"))?;

        if let Some(existing) =
            find_existing_by_reference(&mut transaction, &persisted.transaction.reference).await?
        {
            if existing.payload_fingerprint == persisted.payload_fingerprint {
                let journal_transaction = load_transaction_by_id(&mut transaction, existing.id)
                    .await?
                    .ok_or_else(|| {
                        AppError::unexpected(
                            ErrorCode::Infrastructure,
                            "existing reversal disappeared during retry".to_owned(),
                        )
                    })?;
                transaction
                    .commit()
                    .await
                    .map_err(db_error("commit replayed reversal"))?;

                return Ok(PostTransactionResult {
                    transaction: journal_transaction,
                    was_replayed: true,
                });
            }

            return Err(AppError::conflict(
                ErrorCode::IdempotencyConflict,
                "reference already exists with a different payload".to_owned(),
            ));
        }

        let original_transaction_id = persisted
            .transaction
            .reversal_of_transaction_id
            .ok_or_else(|| {
                AppError::validation(
                    ErrorCode::ReversalNotAllowed,
                    "reversal_of_transaction_id is required for reversals".to_owned(),
                )
            })?;

        let original = load_transaction_by_id(&mut transaction, original_transaction_id)
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

        let already_reversed = sqlx::query_scalar::<_, i64>(
            "select id from journal_transactions where reversal_of_transaction_id = $1 limit 1",
        )
        .bind(original_transaction_id.value())
        .fetch_optional(transaction.as_mut())
        .await
        .map_err(db_error("check existing reversal"))?;

        if already_reversed.is_some() {
            return Err(AppError::conflict(
                ErrorCode::ReversalNotAllowed,
                format!("transaction {original_transaction_id} has already been reversed"),
            ));
        }

        ensure_accounts_exist(&mut transaction, &persisted.transaction.entries).await?;
        insert_transaction(&mut transaction, &persisted).await?;
        insert_entries(&mut transaction, &persisted.transaction.entries).await?;

        transaction
            .commit()
            .await
            .map_err(db_error("commit reversal"))?;

        Ok(PostTransactionResult {
            transaction: persisted.transaction,
            was_replayed: false,
        })
    }

    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<JournalTransaction>, AppError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(db_error("begin get transaction"))?;
        let result = load_transaction_by_id(&mut transaction, transaction_id).await?;
        transaction
            .commit()
            .await
            .map_err(db_error("commit get transaction"))?;
        Ok(result)
    }

    async fn get_transaction_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<JournalTransaction>, AppError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(db_error("begin get transaction by reference"))?;

        let id = sqlx::query_scalar::<_, i64>(
            "select id from journal_transactions where reference = $1 limit 1",
        )
        .bind(reference)
        .fetch_optional(transaction.as_mut())
        .await
        .map_err(db_error("select transaction by reference"))?;

        let result = match id {
            Some(id) => load_transaction_by_id(&mut transaction, TransactionId::new(id)).await?,
            None => None,
        };

        transaction
            .commit()
            .await
            .map_err(db_error("commit get transaction by reference"))?;

        Ok(result)
    }

    async fn get_balance(
        &self,
        account_id: AccountId,
    ) -> Result<Option<AccountBalanceRecord>, AppError> {
        let account_row = sqlx::query(
            r#"
            select account_type::text as account_type
            from accounts
            where id = $1
            "#,
        )
        .bind(account_id.value())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error("select account for balance"))?;

        let Some(account_row) = account_row else {
            return Ok(None);
        };

        let account_type =
            AccountType::from_db_value(&account_row.get::<String, _>("account_type"))?;

        let aggregates = sqlx::query(
            r#"
            select
                coalesce(sum(case when direction = 'debit' then amount_in_cents else 0 end), 0)::bigint as debit_total,
                coalesce(sum(case when direction = 'credit' then amount_in_cents else 0 end), 0)::bigint as credit_total
            from journal_entries
            where account_id = $1
            "#,
        )
        .bind(account_id.value())
        .fetch_one(&self.pool)
        .await
        .map_err(db_error("select balance aggregates"))?;

        let debits = Money::from_minor_units(aggregates.get::<i64, _>("debit_total"))?;
        let credits = Money::from_minor_units(aggregates.get::<i64, _>("credit_total"))?;

        Ok(Some(AccountBalanceRecord {
            account_type,
            snapshot: BalanceSnapshot {
                account_id,
                currency: Currency::Brl,
                debits,
                credits,
                net_in_cents: derive_net_balance(account_type, debits, credits),
            },
        }))
    }

    async fn get_statement(
        &self,
        account_id: AccountId,
        query: &StatementQuery,
    ) -> Result<Option<StatementPage>, AppError> {
        let account_row = sqlx::query(
            r#"
            select account_type::text as account_type
            from accounts
            where id = $1
            "#,
        )
        .bind(account_id.value())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error("select account for statement"))?;

        let Some(account_row) = account_row else {
            return Ok(None);
        };

        let account_type =
            AccountType::from_db_value(&account_row.get::<String, _>("account_type"))?;

        let cursor_created_at = query.cursor.as_ref().map(|cursor| cursor.created_at);
        let cursor_transaction_id = query
            .cursor
            .as_ref()
            .map(|cursor| cursor.transaction_id.value());
        let cursor_entry_id = query.cursor.as_ref().map(|cursor| cursor.entry_id.value());

        let rows = sqlx::query(
            r#"
            select
                e.id as entry_id,
                e.transaction_id,
                e.direction::text as direction,
                e.amount_in_cents,
                e.created_at,
                t.reference,
                t.description
            from journal_entries e
            join journal_transactions t on t.id = e.transaction_id
            where e.account_id = $1
              and ($2::timestamptz is null or e.created_at >= $2)
              and ($3::timestamptz is null or e.created_at <= $3)
              and (
                $4::timestamptz is null
                or (e.created_at, e.transaction_id, e.id) > ($4, $5, $6)
              )
            order by e.created_at asc, e.transaction_id asc, e.id asc
            limit $7
            "#,
        )
        .bind(account_id.value())
        .bind(query.from)
        .bind(query.to)
        .bind(cursor_created_at)
        .bind(cursor_transaction_id)
        .bind(cursor_entry_id)
        .bind((query.limit as i64) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error("select statement rows"))?;

        let (mut running_debits, mut running_credits) = if let Some(cursor) = query.cursor.as_ref()
        {
            let opening = sqlx::query(
                r#"
                select
                    coalesce(sum(case when direction = 'debit' then amount_in_cents else 0 end), 0)::bigint as debit_total,
                    coalesce(sum(case when direction = 'credit' then amount_in_cents else 0 end), 0)::bigint as credit_total
                from journal_entries
                where account_id = $1
                  and (created_at, transaction_id, id) <= ($2, $3, $4)
                "#,
            )
            .bind(account_id.value())
            .bind(cursor.created_at)
            .bind(cursor.transaction_id.value())
            .bind(cursor.entry_id.value())
            .fetch_one(&self.pool)
            .await
            .map_err(db_error("select opening balance from cursor"))?;

            (
                Money::from_minor_units(opening.get::<i64, _>("debit_total"))?,
                Money::from_minor_units(opening.get::<i64, _>("credit_total"))?,
            )
        } else if let Some(from) = query.from {
            let opening = sqlx::query(
                r#"
                select
                    coalesce(sum(case when direction = 'debit' then amount_in_cents else 0 end), 0)::bigint as debit_total,
                    coalesce(sum(case when direction = 'credit' then amount_in_cents else 0 end), 0)::bigint as credit_total
                from journal_entries
                where account_id = $1
                  and created_at < $2
                "#,
            )
            .bind(account_id.value())
            .bind(from)
            .fetch_one(&self.pool)
            .await
            .map_err(db_error("select opening balance from from-date"))?;

            (
                Money::from_minor_units(opening.get::<i64, _>("debit_total"))?,
                Money::from_minor_units(opening.get::<i64, _>("credit_total"))?,
            )
        } else {
            (Money::zero(), Money::zero())
        };

        let has_more = rows.len() > query.limit;
        let mut entries = Vec::new();
        let mut next_cursor = None;
        let mut last_returned_cursor = None;

        for row in rows.into_iter().take(query.limit) {
            let cursor = StatementCursor {
                created_at: row.get("created_at"),
                transaction_id: TransactionId::new(row.get("transaction_id")),
                entry_id: EntryId::new(row.get("entry_id")),
            };

            let direction = EntryDirection::from_db_value(&row.get::<String, _>("direction"))?;
            let amount = Money::from_minor_units(row.get("amount_in_cents"))?;
            match direction {
                EntryDirection::Debit => running_debits += amount,
                EntryDirection::Credit => running_credits += amount,
            }
            last_returned_cursor = Some(cursor.clone());

            entries.push(StatementEntry {
                entry_id: cursor.entry_id,
                transaction_id: cursor.transaction_id,
                reference: row.get("reference"),
                description: row.get("description"),
                direction,
                amount,
                running_balance_in_cents: derive_net_balance(
                    account_type,
                    running_debits,
                    running_credits,
                ),
                created_at: row.get("created_at"),
            });
        }

        if has_more {
            next_cursor = last_returned_cursor
                .as_ref()
                .map(encode_statement_cursor)
                .transpose()?;
        }

        Ok(Some(StatementPage {
            entries,
            next_cursor,
        }))
    }
}

struct ExistingReference {
    id: TransactionId,
    payload_fingerprint: String,
}

async fn find_existing_by_reference(
    transaction: &mut Transaction<'_, Postgres>,
    reference: &str,
) -> Result<Option<ExistingReference>, AppError> {
    let row = sqlx::query(
        r#"
        select id, payload_fingerprint
        from journal_transactions
        where reference = $1
        limit 1
        "#,
    )
    .bind(reference)
    .fetch_optional(transaction.as_mut())
    .await
    .map_err(db_error("select existing transaction by reference"))?;

    Ok(row.map(|row| ExistingReference {
        id: TransactionId::new(row.get("id")),
        payload_fingerprint: row.get("payload_fingerprint"),
    }))
}

async fn ensure_accounts_exist(
    transaction: &mut Transaction<'_, Postgres>,
    entries: &[JournalEntry],
) -> Result<(), AppError> {
    let unique_ids = entries
        .iter()
        .map(|entry| entry.account_id.value())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let rows = sqlx::query_scalar::<_, i64>("select id from accounts where id = any($1)")
        .bind(&unique_ids)
        .fetch_all(transaction.as_mut())
        .await
        .map_err(db_error("verify account existence"))?;

    if rows.len() != unique_ids.len() {
        error!("transaction rejected because at least one account is missing");
        return Err(AppError::not_found(
            ErrorCode::AccountNotFound,
            "one or more referenced accounts do not exist".to_owned(),
        ));
    }

    Ok(())
}

async fn insert_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    persisted: &PersistedTransaction,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into journal_transactions (
            id,
            reference,
            payload_fingerprint,
            description,
            reversal_of_transaction_id,
            created_at
        )
        values ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(persisted.transaction.id.value())
    .bind(&persisted.transaction.reference)
    .bind(&persisted.payload_fingerprint)
    .bind(persisted.transaction.description.as_deref())
    .bind(
        persisted
            .transaction
            .reversal_of_transaction_id
            .map(|id| id.value()),
    )
    .bind(persisted.transaction.created_at)
    .execute(transaction.as_mut())
    .await
    .map_err(db_error("insert journal transaction"))?;

    info!(
        reference = persisted.transaction.reference,
        "posted journal transaction"
    );

    Ok(())
}

async fn insert_entries(
    transaction: &mut Transaction<'_, Postgres>,
    entries: &[JournalEntry],
) -> Result<(), AppError> {
    for entry in entries {
        sqlx::query(
            r#"
            insert into journal_entries (
                id,
                transaction_id,
                account_id,
                direction,
                amount_in_cents,
                created_at
            )
            values ($1, $2, $3, $4::entry_direction, $5, $6)
            "#,
        )
        .bind(entry.id.value())
        .bind(entry.transaction_id.value())
        .bind(entry.account_id.value())
        .bind(entry.direction.as_db_value())
        .bind(entry.amount.amount_in_cents())
        .bind(entry.created_at)
        .execute(transaction.as_mut())
        .await
        .map_err(db_error("insert journal entry"))?;
    }

    Ok(())
}

async fn load_transaction_by_id(
    transaction: &mut Transaction<'_, Postgres>,
    transaction_id: TransactionId,
) -> Result<Option<JournalTransaction>, AppError> {
    let header = sqlx::query(
        r#"
        select id, reference, description, reversal_of_transaction_id, created_at
        from journal_transactions
        where id = $1
        "#,
    )
    .bind(transaction_id.value())
    .fetch_optional(transaction.as_mut())
    .await
    .map_err(db_error("load transaction header"))?;

    let Some(header) = header else {
        return Ok(None);
    };

    let entry_rows = sqlx::query(
        r#"
        select id, transaction_id, account_id, direction::text as direction, amount_in_cents, created_at
        from journal_entries
        where transaction_id = $1
        order by created_at asc, transaction_id asc, id asc
        "#,
    )
    .bind(transaction_id.value())
    .fetch_all(transaction.as_mut())
    .await
    .map_err(db_error("load transaction entries"))?;

    let mut entries = Vec::with_capacity(entry_rows.len());

    for row in entry_rows {
        entries.push(JournalEntry {
            id: EntryId::new(row.get("id")),
            transaction_id: TransactionId::new(row.get("transaction_id")),
            account_id: AccountId::new(row.get("account_id")),
            direction: EntryDirection::from_db_value(&row.get::<String, _>("direction"))?,
            amount: Money::from_minor_units(row.get("amount_in_cents"))?,
            created_at: row.get("created_at"),
        });
    }

    Ok(Some(JournalTransaction {
        id: TransactionId::new(header.get("id")),
        reference: header.get("reference"),
        description: header.get("description"),
        reversal_of_transaction_id: header
            .get::<Option<i64>, _>("reversal_of_transaction_id")
            .map(TransactionId::new),
        created_at: header.get("created_at"),
        entries,
    }))
}

fn db_error(context: &'static str) -> impl Fn(sqlx::Error) -> AppError {
    move |error| {
        AppError::unexpected(
            ErrorCode::Infrastructure,
            format!("failed to {context}: {error}"),
        )
    }
}
