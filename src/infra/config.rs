use std::{env, net::SocketAddr};

use crate::shared::errors::{AppError, ErrorCode};

const DATABASE_URL_ENV: &str = "DATABASE_URL";
const NODE_ID_ENV: &str = "NODE_ID";
const HTTP_ADDR_ENV: &str = "HTTP_ADDR";
const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:3000";

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub http_addr: SocketAddr,
    pub node_id: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let database_url = required_env(DATABASE_URL_ENV)?;

        let http_addr = env::var(HTTP_ADDR_ENV)
            .unwrap_or_else(|_| DEFAULT_HTTP_ADDR.to_owned())
            .parse()
            .map_err(|error| {
                AppError::validation(
                    ErrorCode::InvalidConfiguration,
                    format!("{HTTP_ADDR_ENV} must be a valid socket address: {error}"),
                )
            })?;

        let node_id = required_env(NODE_ID_ENV)?.parse().map_err(|error| {
            AppError::validation(
                ErrorCode::InvalidConfiguration,
                format!("{NODE_ID_ENV} must be a valid integer: {error}"),
            )
        })?;

        Ok(Self {
            database_url,
            http_addr,
            node_id,
        })
    }
}

fn required_env(name: &'static str) -> Result<String, AppError> {
    env::var(name).map_err(|_| {
        AppError::validation(
            ErrorCode::InvalidConfiguration,
            format!("{name} is required"),
        )
    })
}
