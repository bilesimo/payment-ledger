use chrono::Utc;
use payment_ledger::{
    modules::accounts::{
        domain::{Account, AccountType},
        dto::CreateAccountRequest,
        mapper,
    },
    shared::{ids::AccountId, money::Currency},
};

#[test]
fn maps_create_account_request() {
    let command = mapper::to_create_account(CreateAccountRequest {
        name: Some("cash".to_owned()),
        account_type: AccountType::Asset,
    });

    assert_eq!(command.name.as_deref(), Some("cash"));
    assert_eq!(command.account_type, AccountType::Asset);
}

#[test]
fn maps_account_to_response() {
    let created_at = Utc::now();
    let response = mapper::to_response(Account {
        id: AccountId::new(42),
        name: Some("cash".to_owned()),
        account_type: AccountType::Asset,
        currency: Currency::Brl,
        created_at,
    });

    assert_eq!(response.account_id, AccountId::new(42));
    assert_eq!(response.name.as_deref(), Some("cash"));
    assert_eq!(response.account_type, AccountType::Asset);
    assert_eq!(response.currency, Currency::Brl);
    assert_eq!(response.created_at, created_at);
}
