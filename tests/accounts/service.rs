use std::sync::Arc;

use payment_ledger::{
    modules::accounts::{
        domain::{AccountType, CreateAccount},
        service::AccountService,
        store::PgAccountStore,
    },
    shared::{
        errors::ErrorCode,
        ids::{AccountId, SnowflakeGenerator},
    },
};
use serial_test::serial;

use crate::support::setup_pool;

#[tokio::test]
#[serial]
async fn create_account_trims_blank_name() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let service = AccountService::new(
        PgAccountStore::new(pool),
        Arc::new(SnowflakeGenerator::new(0).expect("generator should build")),
    );

    let account = service
        .create_account(CreateAccount {
            name: Some("   ".to_owned()),
            account_type: AccountType::Asset,
        })
        .await
        .expect("account creation should succeed");

    assert_eq!(account.name, None);
}

#[tokio::test]
#[serial]
async fn get_account_returns_not_found() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let service = AccountService::new(
        PgAccountStore::new(pool),
        Arc::new(SnowflakeGenerator::new(0).expect("generator should build")),
    );

    let error = service
        .get_account(AccountId::new(999999))
        .await
        .expect_err("missing account should return an error");

    assert_eq!(error.code(), ErrorCode::AccountNotFound);
}
