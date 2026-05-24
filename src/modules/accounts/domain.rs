use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::{
    errors::{AppError, ErrorCode},
    ids::AccountId,
    money::Currency,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    Asset,
    Liability,
    Revenue,
    Expense,
}

impl AccountType {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Revenue => "revenue",
            Self::Expense => "expense",
        }
    }

    pub fn from_db_value(value: &str) -> Result<Self, AppError> {
        match value {
            "asset" => Ok(Self::Asset),
            "liability" => Ok(Self::Liability),
            "revenue" => Ok(Self::Revenue),
            "expense" => Ok(Self::Expense),
            unknown => Err(AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("unknown account type stored in database: {unknown}"),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateAccount {
    pub name: Option<String>,
    pub account_type: AccountType,
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: AccountId,
    pub name: Option<String>,
    pub account_type: AccountType,
    pub currency: Currency,
    pub created_at: DateTime<Utc>,
}

impl Account {
    pub fn new(
        id: AccountId,
        name: Option<String>,
        account_type: AccountType,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            account_type,
            currency: Currency::Brl,
            created_at,
        }
    }
}
