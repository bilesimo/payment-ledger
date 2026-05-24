use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    modules::accounts::domain::AccountType,
    shared::{
        errors::{AppError, ErrorCode},
        ids::{AccountId, EntryId, TransactionId},
        money::{Currency, Money},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryDirection {
    Debit,
    Credit,
}

impl EntryDirection {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }

    pub fn from_db_value(value: &str) -> Result<Self, AppError> {
        match value {
            "debit" => Ok(Self::Debit),
            "credit" => Ok(Self::Credit),
            unknown => Err(AppError::unexpected(
                ErrorCode::Infrastructure,
                format!("unknown entry direction stored in database: {unknown}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryDraft {
    pub account_id: AccountId,
    pub direction: EntryDirection,
    pub amount: Money,
}

#[derive(Debug, Clone)]
pub struct PostTransaction {
    pub reference: String,
    pub description: Option<String>,
    pub entries: Vec<EntryDraft>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    pub id: EntryId,
    pub transaction_id: TransactionId,
    pub account_id: AccountId,
    pub direction: EntryDirection,
    pub amount: Money,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalTransaction {
    pub id: TransactionId,
    pub reference: String,
    pub description: Option<String>,
    pub reversal_of_transaction_id: Option<TransactionId>,
    pub entries: Vec<JournalEntry>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceSnapshot {
    pub account_id: AccountId,
    pub currency: Currency,
    pub debits: Money,
    pub credits: Money,
    pub net_in_cents: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatementEntry {
    pub entry_id: EntryId,
    pub transaction_id: TransactionId,
    pub reference: String,
    pub description: Option<String>,
    pub direction: EntryDirection,
    pub amount: Money,
    pub running_balance_in_cents: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StatementPage {
    pub entries: Vec<StatementEntry>,
    pub next_cursor: Option<String>,
}

pub fn normalize_reference(reference: String) -> Result<String, AppError> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            ErrorCode::InvalidRequest,
            "reference must be present".to_owned(),
        ));
    }

    Ok(trimmed.to_owned())
}

pub fn normalize_description(description: Option<String>) -> Option<String> {
    description.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

pub fn validate_posting(entries: &[EntryDraft]) -> Result<(), AppError> {
    if entries.len() < 2 {
        return Err(AppError::validation(
            ErrorCode::InvalidRequest,
            "a journal transaction must contain at least two entries".to_owned(),
        ));
    }

    let mut total_debits = Money::zero();
    let mut total_credits = Money::zero();

    for entry in entries {
        if entry.amount.is_zero() {
            return Err(AppError::validation(
                ErrorCode::InvalidMoneyAmount,
                "journal entries must use a positive amount".to_owned(),
            ));
        }

        match entry.direction {
            EntryDirection::Debit => total_debits += entry.amount,
            EntryDirection::Credit => total_credits += entry.amount,
        }
    }

    if total_debits != total_credits {
        return Err(AppError::validation(
            ErrorCode::UnbalancedTransaction,
            "total debits must equal total credits".to_owned(),
        ));
    }

    Ok(())
}

pub fn derive_net_balance(account_type: AccountType, debits: Money, credits: Money) -> i64 {
    match account_type {
        AccountType::Asset | AccountType::Expense => {
            debits.amount_in_cents() - credits.amount_in_cents()
        }
        AccountType::Liability | AccountType::Revenue => {
            credits.amount_in_cents() - debits.amount_in_cents()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EntryDirection, EntryDraft, derive_net_balance, validate_posting};
    use crate::{
        modules::accounts::domain::AccountType,
        shared::{ids::AccountId, money::Money},
    };

    #[test]
    fn rejects_unbalanced_transactions() {
        let entries = vec![
            EntryDraft {
                account_id: AccountId::new(1),
                direction: EntryDirection::Debit,
                amount: Money::from_minor_units(100).expect("amount should be valid"),
            },
            EntryDraft {
                account_id: AccountId::new(2),
                direction: EntryDirection::Credit,
                amount: Money::from_minor_units(50).expect("amount should be valid"),
            },
        ];

        assert!(validate_posting(&entries).is_err());
    }

    #[test]
    fn accepts_balanced_transactions() {
        let entries = vec![
            EntryDraft {
                account_id: AccountId::new(1),
                direction: EntryDirection::Debit,
                amount: Money::from_minor_units(100).expect("amount should be valid"),
            },
            EntryDraft {
                account_id: AccountId::new(2),
                direction: EntryDirection::Credit,
                amount: Money::from_minor_units(100).expect("amount should be valid"),
            },
        ];

        assert!(validate_posting(&entries).is_ok());
    }

    #[test]
    fn derives_liability_balance_from_credits_minus_debits() {
        let debits = Money::from_minor_units(40).expect("amount should be valid");
        let credits = Money::from_minor_units(100).expect("amount should be valid");

        assert_eq!(
            derive_net_balance(AccountType::Liability, debits, credits),
            60
        );
    }
}
