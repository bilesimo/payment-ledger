use std::env;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, header},
    response::Response,
};
use payment_ledger::{build_app, infra::config::Config};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
}

pub async fn setup_pool() -> Option<PgPool> {
    let database_url = test_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .ok()?;

    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    sqlx::query(
        "truncate table journal_entries, journal_transactions, accounts restart identity cascade",
    )
    .execute(&pool)
    .await
    .ok()?;

    Some(pool)
}

pub async fn setup_app() -> Option<Router> {
    let database_url = test_database_url()?;
    let pool = setup_pool().await?;
    pool.close().await;

    let config = Config {
        database_url,
        http_addr: "127.0.0.1:3000".parse().expect("socket should parse"),
        node_id: 0,
    };

    build_app(&config).await.ok()
}

pub async fn send_request(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> Response {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&body).expect("json body should serialize"))
        }
        None => Body::empty(),
    };

    app.clone()
        .oneshot(request.body(body).expect("request should build"))
        .await
        .expect("request should succeed")
}

pub async fn read_json<T: DeserializeOwned>(response: Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    serde_json::from_slice(&bytes).expect("response body should deserialize")
}

fn test_database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL")
        .ok()
        .or_else(|| env::var("DATABASE_URL").ok())
}
