use std::sync::Arc;

use payment_ledger::{
    modules::{
        accounts::{
            domain::{AccountType, CreateAccount},
            service::AccountService,
            store::PgAccountStore,
        },
        journal::{
            domain::{EntryDirection, EntryDraft, PostTransaction},
            service::JournalService,
            store::PgJournalStore,
        },
    },
    shared::{ids::SnowflakeGenerator, money::Money},
};
use serial_test::serial;

use crate::support::setup_pool;

#[tokio::test]
#[serial]
async fn post_transaction_is_idempotent() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let generator = Arc::new(SnowflakeGenerator::new(0).expect("generator should build"));
    let account_service = AccountService::new(PgAccountStore::new(pool.clone()), generator.clone());
    let journal_service = JournalService::new(PgJournalStore::new(pool), generator);

    let cash = account_service
        .create_account(CreateAccount {
            name: Some("cash".to_owned()),
            account_type: AccountType::Asset,
        })
        .await
        .expect("cash account creation should succeed");
    let payable = account_service
        .create_account(CreateAccount {
            name: Some("merchant payable".to_owned()),
            account_type: AccountType::Liability,
        })
        .await
        .expect("payable account creation should succeed");

    let payload = || PostTransaction {
        reference: "payment-id-1".to_owned(),
        description: Some("merchant payout".to_owned()),
        entries: vec![
            EntryDraft {
                account_id: cash.id,
                direction: EntryDirection::Debit,
                amount: Money::from_minor_units(1_000).expect("money should be valid"),
            },
            EntryDraft {
                account_id: payable.id,
                direction: EntryDirection::Credit,
                amount: Money::from_minor_units(1_000).expect("money should be valid"),
            },
        ],
    };

    let first = journal_service
        .post_transaction(payload())
        .await
        .expect("first post should succeed");
    let second = journal_service
        .post_transaction(payload())
        .await
        .expect("replayed post should succeed");

    assert!(!first.was_replayed);
    assert!(second.was_replayed);
    assert_eq!(first.transaction.id, second.transaction.id);
}

#[tokio::test]
#[serial]
async fn reverse_transaction_creates_inverse_entries() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let generator = Arc::new(SnowflakeGenerator::new(0).expect("generator should build"));
    let account_service = AccountService::new(PgAccountStore::new(pool.clone()), generator.clone());
    let journal_service = JournalService::new(PgJournalStore::new(pool), generator);

    let asset = account_service
        .create_account(CreateAccount {
            name: Some("cash".to_owned()),
            account_type: AccountType::Asset,
        })
        .await
        .expect("asset account creation should succeed");
    let liability = account_service
        .create_account(CreateAccount {
            name: Some("payable".to_owned()),
            account_type: AccountType::Liability,
        })
        .await
        .expect("liability account creation should succeed");

    let posted = journal_service
        .post_transaction(PostTransaction {
            reference: "payment-id-2".to_owned(),
            description: Some("merchant payout".to_owned()),
            entries: vec![
                EntryDraft {
                    account_id: asset.id,
                    direction: EntryDirection::Debit,
                    amount: Money::from_minor_units(500).expect("money should be valid"),
                },
                EntryDraft {
                    account_id: liability.id,
                    direction: EntryDirection::Credit,
                    amount: Money::from_minor_units(500).expect("money should be valid"),
                },
            ],
        })
        .await
        .expect("post should succeed");

    let reversal = journal_service
        .reverse_transaction(
            posted.transaction.id,
            "payment-id-2-reversal".to_owned(),
            None,
        )
        .await
        .expect("reversal should succeed");

    assert_eq!(
        reversal.transaction.reversal_of_transaction_id,
        Some(posted.transaction.id)
    );
    assert_eq!(
        reversal.transaction.entries[0].direction,
        EntryDirection::Credit
    );
    assert_eq!(
        reversal.transaction.entries[1].direction,
        EntryDirection::Debit
    );
}
