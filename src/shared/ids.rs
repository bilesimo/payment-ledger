use std::{
    fmt::{Display, Formatter},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::shared::errors::{AppError, ErrorCode};

const CUSTOM_EPOCH_MS: u64 = 1_704_067_200_000;
const NODE_ID_BITS: u8 = 10;
const SEQUENCE_BITS: u8 = 12;
const MAX_NODE_ID: u16 = (1 << NODE_ID_BITS) - 1;
const MAX_SEQUENCE: u16 = (1 << SEQUENCE_BITS) - 1;

#[derive(Debug)]
pub struct SnowflakeGenerator {
    node_id: u16,
    state: AtomicU64,
}

impl SnowflakeGenerator {
    pub fn new(node_id: u16) -> Result<Self, AppError> {
        if node_id > MAX_NODE_ID {
            return Err(AppError::validation(
                ErrorCode::InvalidConfiguration,
                format!("NODE_ID must be between 0 and {MAX_NODE_ID}"),
            ));
        }

        Ok(Self {
            node_id,
            state: AtomicU64::new(0),
        })
    }

    pub fn next_account_id(&self) -> Result<AccountId, AppError> {
        self.next_id().map(AccountId::new)
    }

    pub fn next_transaction_id(&self) -> Result<TransactionId, AppError> {
        self.next_id().map(TransactionId::new)
    }

    pub fn next_entry_id(&self) -> Result<EntryId, AppError> {
        self.next_id().map(EntryId::new)
    }

    fn next_id(&self) -> Result<i64, AppError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                AppError::unexpected(
                    ErrorCode::Infrastructure,
                    format!("system clock is before unix epoch: {error}"),
                )
            })?
            .as_millis() as u64;

        if now_ms < CUSTOM_EPOCH_MS {
            return Err(AppError::unexpected(
                ErrorCode::Infrastructure,
                "system clock is before configured snowflake epoch".to_owned(),
            ));
        }

        let timestamp_ms = now_ms - CUSTOM_EPOCH_MS;

        loop {
            let current = self.state.load(Ordering::Relaxed);
            let previous_timestamp_ms = current >> SEQUENCE_BITS;
            let previous_sequence = (current & MAX_SEQUENCE as u64) as u16;

            if timestamp_ms < previous_timestamp_ms {
                return Err(AppError::unexpected(
                    ErrorCode::Infrastructure,
                    "system clock moved backwards".to_owned(),
                ));
            }

            let (next_timestamp_ms, next_sequence) = if timestamp_ms == previous_timestamp_ms {
                if previous_sequence == MAX_SEQUENCE {
                    return Err(AppError::unexpected(
                        ErrorCode::Infrastructure,
                        "snowflake sequence exhausted within the same millisecond".to_owned(),
                    ));
                }

                (timestamp_ms, previous_sequence + 1)
            } else {
                (timestamp_ms, 0)
            };

            let next_state = (next_timestamp_ms << SEQUENCE_BITS) | next_sequence as u64;

            if self
                .state
                .compare_exchange(current, next_state, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let id = (next_timestamp_ms << (NODE_ID_BITS + SEQUENCE_BITS))
                    | ((self.node_id as u64) << SEQUENCE_BITS)
                    | next_sequence as u64;

                return Ok(id as i64);
            }
        }
    }
}

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
        )]
        #[serde(transparent)]
        pub struct $name(i64);

        impl $name {
            pub fn new(value: i64) -> Self {
                Self(value)
            }

            pub fn value(self) -> i64 {
                self.0
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self::new(value)
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

typed_id!(AccountId);
typed_id!(TransactionId);
typed_id!(EntryId);
