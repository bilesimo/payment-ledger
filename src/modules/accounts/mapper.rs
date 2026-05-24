use crate::modules::accounts::{
    domain::{Account, CreateAccount},
    dto::{AccountResponse, CreateAccountRequest},
};

pub fn to_create_account(request: CreateAccountRequest) -> CreateAccount {
    CreateAccount {
        name: request.name,
        account_type: request.account_type,
    }
}

pub fn to_response(account: Account) -> AccountResponse {
    AccountResponse {
        account_id: account.id,
        name: account.name,
        account_type: account.account_type,
        currency: account.currency,
        created_at: account.created_at,
    }
}
