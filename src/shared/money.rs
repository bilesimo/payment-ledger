use std::ops::{Add, AddAssign};

use serde::{Deserialize, Serialize};

use crate::shared::errors::{AppError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Brl,
}

impl Currency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brl => "BRL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Money {
    amount_in_cents: i64,
}

impl Money {
    pub fn from_minor_units(amount_in_cents: i64) -> Result<Self, AppError> {
        if amount_in_cents < 0 {
            return Err(AppError::validation(
                ErrorCode::InvalidMoneyAmount,
                "money amount must be non-negative".to_owned(),
            ));
        }

        Ok(Self { amount_in_cents })
    }

    pub fn zero() -> Self {
        Self { amount_in_cents: 0 }
    }

    pub fn amount_in_cents(self) -> i64 {
        self.amount_in_cents
    }

    pub fn is_zero(self) -> bool {
        self.amount_in_cents == 0
    }
}

impl Add for Money {
    type Output = Money;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            amount_in_cents: self.amount_in_cents + rhs.amount_in_cents,
        }
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.amount_in_cents += rhs.amount_in_cents;
    }
}

#[cfg(test)]
mod tests {
    use super::Money;

    #[test]
    fn rejects_negative_amounts() {
        assert!(Money::from_minor_units(-1).is_err());
    }

    #[test]
    fn accepts_zero_for_read_models() {
        let money = Money::from_minor_units(0).expect("zero balance should be valid");
        assert_eq!(money.amount_in_cents(), 0);
    }
}
