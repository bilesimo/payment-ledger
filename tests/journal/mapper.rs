use chrono::Utc;
use payment_ledger::{
    modules::journal::{
        domain::{BalanceSnapshot, EntryDirection, StatementEntry, StatementPage},
        dto::{CreateTransactionEntryRequest, CreateTransactionRequest},
        mapper,
    },
    shared::{
        errors::ErrorCode,
        ids::{AccountId, EntryId, TransactionId},
        money::{Currency, Money},
    },
};

#[test]
fn maps_transaction_request_to_domain() {
    let created = mapper::to_post_transaction(CreateTransactionRequest {
        reference: "payment-id-1".to_owned(),
        description: Some("merchant payout".to_owned()),
        entries: vec![
            CreateTransactionEntryRequest {
                account_id: AccountId::new(10),
                direction: EntryDirection::Debit,
                amount_in_cents: 100,
            },
            CreateTransactionEntryRequest {
                account_id: AccountId::new(20),
                direction: EntryDirection::Credit,
                amount_in_cents: 100,
            },
        ],
    })
    .expect("mapping should succeed");

    assert_eq!(created.reference, "payment-id-1");
    assert_eq!(created.entries.len(), 2);
    assert_eq!(created.entries[0].amount.amount_in_cents(), 100);
}

#[test]
fn rejects_empty_transaction_entries() {
    let error = mapper::to_post_transaction(CreateTransactionRequest {
        reference: "payment-id-1".to_owned(),
        description: None,
        entries: Vec::new(),
    })
    .expect_err("empty entries should fail");

    assert_eq!(error.code(), ErrorCode::InvalidRequest);
}

#[test]
fn maps_statement_page_to_response() {
    let created_at = Utc::now();
    let response = mapper::to_statement_response(StatementPage {
        entries: vec![StatementEntry {
            entry_id: EntryId::new(1),
            transaction_id: TransactionId::new(2),
            reference: "payment-id-1".to_owned(),
            description: Some("merchant payout".to_owned()),
            direction: EntryDirection::Debit,
            amount: Money::from_minor_units(500).expect("money should be valid"),
            running_balance_in_cents: 500,
            created_at,
        }],
        next_cursor: Some("cursor-1".to_owned()),
    });

    assert_eq!(response.entries.len(), 1);
    assert_eq!(response.entries[0].reference, "payment-id-1");
    assert_eq!(response.entries[0].amount_in_cents, 500);
    assert_eq!(response.next_cursor.as_deref(), Some("cursor-1"));
}

#[test]
fn maps_balance_snapshot_to_response() {
    let response = mapper::to_balance_response(BalanceSnapshot {
        account_id: AccountId::new(42),
        currency: Currency::Brl,
        debits: Money::from_minor_units(900).expect("money should be valid"),
        credits: Money::from_minor_units(100).expect("money should be valid"),
        net_in_cents: 800,
    });

    assert_eq!(response.account_id, AccountId::new(42));
    assert_eq!(response.currency, "BRL");
    assert_eq!(response.net_in_cents, 800);
}
