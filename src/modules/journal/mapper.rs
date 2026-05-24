use crate::{
    modules::journal::{
        domain::{BalanceSnapshot, EntryDraft, JournalTransaction, PostTransaction, StatementPage},
        dto::{
            BalanceResponse, CreateTransactionRequest, JournalEntryResponse,
            JournalTransactionResponse, PostTransactionResponse, StatementEntryResponse,
            StatementResponse,
        },
    },
    shared::{
        errors::{AppError, ErrorCode},
        money::Money,
    },
};

pub fn to_post_transaction(request: CreateTransactionRequest) -> Result<PostTransaction, AppError> {
    let entries = request
        .entries
        .into_iter()
        .map(|entry| {
            Ok(EntryDraft {
                account_id: entry.account_id,
                direction: entry.direction,
                amount: Money::from_minor_units(entry.amount_in_cents)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    if entries.is_empty() {
        return Err(AppError::validation(
            ErrorCode::InvalidRequest,
            "entries must not be empty".to_owned(),
        ));
    }

    Ok(PostTransaction {
        reference: request.reference,
        description: request.description,
        entries,
    })
}

pub fn to_transaction_response(transaction: JournalTransaction) -> JournalTransactionResponse {
    JournalTransactionResponse {
        transaction_id: transaction.id,
        reference: transaction.reference,
        description: transaction.description,
        reversal_of_transaction_id: transaction.reversal_of_transaction_id,
        created_at: transaction.created_at,
        entries: transaction
            .entries
            .into_iter()
            .map(|entry| JournalEntryResponse {
                entry_id: entry.id,
                account_id: entry.account_id,
                direction: entry.direction,
                amount_in_cents: entry.amount.amount_in_cents(),
                created_at: entry.created_at,
            })
            .collect(),
    }
}

pub fn to_post_response(transaction: JournalTransaction) -> PostTransactionResponse {
    PostTransactionResponse {
        transaction: to_transaction_response(transaction),
    }
}

pub fn to_balance_response(snapshot: BalanceSnapshot) -> BalanceResponse {
    BalanceResponse {
        account_id: snapshot.account_id,
        currency: snapshot.currency.as_str(),
        debits_in_cents: snapshot.debits.amount_in_cents(),
        credits_in_cents: snapshot.credits.amount_in_cents(),
        net_in_cents: snapshot.net_in_cents,
    }
}

pub fn to_statement_response(page: StatementPage) -> StatementResponse {
    StatementResponse {
        entries: page
            .entries
            .into_iter()
            .map(|entry| StatementEntryResponse {
                entry_id: entry.entry_id,
                transaction_id: entry.transaction_id,
                reference: entry.reference,
                description: entry.description,
                direction: entry.direction,
                amount_in_cents: entry.amount.amount_in_cents(),
                running_balance_in_cents: entry.running_balance_in_cents,
                created_at: entry.created_at,
            })
            .collect(),
        next_cursor: page.next_cursor,
    }
}
