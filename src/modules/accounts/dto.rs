use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    modules::accounts::domain::AccountType,
    shared::{ids::AccountId, money::Currency},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateAccountRequest {
    pub name: Option<String>,
    pub account_type: AccountType,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountResponse {
    pub account_id: AccountId,
    pub name: Option<String>,
    pub account_type: AccountType,
    pub currency: Currency,
    pub created_at: DateTime<Utc>,
}
