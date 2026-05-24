use std::env;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use payment_ledger::{build_app, infra::config::Config};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
struct AccountResponse {
    account_id: i64,
    name: Option<String>,
    account_type: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct PostTransactionResponse {
    transaction: JournalTransactionResponse,
}

#[derive(Debug, Deserialize)]
struct JournalTransactionResponse {
    transaction_id: i64,
    reference: String,
    description: Option<String>,
    reversal_of_transaction_id: Option<i64>,
    entries: Vec<JournalEntryResponse>,
}

#[derive(Debug, Deserialize)]
struct JournalEntryResponse {
    account_id: i64,
    direction: String,
    amount_in_cents: i64,
}

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    account_id: i64,
    currency: String,
    debits_in_cents: i64,
    credits_in_cents: i64,
    net_in_cents: i64,
}

#[derive(Debug, Deserialize)]
struct StatementResponse {
    entries: Vec<StatementEntryResponse>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StatementEntryResponse {
    reference: String,
    direction: String,
    amount_in_cents: i64,
    running_balance_in_cents: i64,
}

async fn setup_app() -> Option<Router> {
    let database_url = test_database_url()?;
    reset_database(&database_url).await?;

    let config = Config {
        database_url,
        http_addr: "127.0.0.1:3000".parse().expect("socket should parse"),
        node_id: 0,
    };

    build_app(&config).await.ok()
}

fn test_database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL")
        .ok()
        .or_else(|| env::var("DATABASE_URL").ok())
}

async fn reset_database(database_url: &str) -> Option<()> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .ok()?;

    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    sqlx::query(
        "truncate table journal_entries, journal_transactions, accounts restart identity cascade",
    )
    .execute(&pool)
    .await
    .ok()?;
    pool.close().await;

    Some(())
}

async fn send_request(app: &Router, method: Method, uri: &str, body: Option<Value>) -> Response {
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

async fn read_json<T: DeserializeOwned>(response: Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    serde_json::from_slice(&bytes).expect("response body should deserialize")
}

async fn create_account(app: &Router, name: &str, account_type: &str) -> AccountResponse {
    let response = send_request(
        app,
        Method::POST,
        "/accounts",
        Some(json!({
            "name": name,
            "account_type": account_type,
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    read_json(response).await
}

async fn post_transaction(
    app: &Router,
    reference: &str,
    debit_account_id: i64,
    credit_account_id: i64,
    amount_in_cents: i64,
) -> (StatusCode, PostTransactionResponse) {
    let response = send_request(
        app,
        Method::POST,
        "/journal/transactions",
        Some(json!({
            "reference": reference,
            "description": "merchant payout",
            "entries": [
                {
                    "account_id": debit_account_id,
                    "direction": "debit",
                    "amount_in_cents": amount_in_cents,
                },
                {
                    "account_id": credit_account_id,
                    "direction": "credit",
                    "amount_in_cents": amount_in_cents,
                }
            ]
        })),
    )
    .await;

    let status = response.status();
    let body = read_json(response).await;
    (status, body)
}

#[tokio::test]
#[serial]
async fn post_accounts_returns_created_account() {
    let Some(app) = setup_app().await else {
        return;
    };

    let account = create_account(&app, "cash", "asset").await;

    assert!(account.account_id > 0);
    assert_eq!(account.name.as_deref(), Some("cash"));
    assert_eq!(account.account_type, "asset");
    assert_eq!(account.currency, "BRL");
}

#[tokio::test]
#[serial]
async fn get_accounts_account_id_returns_account() {
    let Some(app) = setup_app().await else {
        return;
    };

    let created = create_account(&app, "merchant payable", "liability").await;
    let response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}", created.account_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let account: AccountResponse = read_json(response).await;
    assert_eq!(account.account_id, created.account_id);
    assert_eq!(account.name.as_deref(), Some("merchant payable"));
    assert_eq!(account.account_type, "liability");
}

#[tokio::test]
#[serial]
async fn get_accounts_account_id_returns_not_found_for_unknown_account() {
    let Some(app) = setup_app().await else {
        return;
    };

    let response = send_request(&app, Method::GET, "/accounts/999999", None).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let error: ErrorResponse = read_json(response).await;
    assert_eq!(error.code, "account_not_found");
    assert!(error.message.contains("999999"));
}

#[tokio::test]
#[serial]
async fn post_journal_transactions_is_idempotent_by_reference() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;

    let (first_status, first) = post_transaction(
        &app,
        "payment-id-1",
        asset.account_id,
        liability.account_id,
        1_000,
    )
    .await;
    let (second_status, second) = post_transaction(
        &app,
        "payment-id-1",
        asset.account_id,
        liability.account_id,
        1_000,
    )
    .await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(
        first.transaction.transaction_id,
        second.transaction.transaction_id
    );
    assert_eq!(first.transaction.reference, "payment-id-1");
    assert_eq!(first.transaction.entries.len(), 2);
}

#[tokio::test]
#[serial]
async fn post_journal_transactions_returns_validation_error_for_unbalanced_payload() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;

    let response = send_request(
        &app,
        Method::POST,
        "/journal/transactions",
        Some(json!({
            "reference": "payment-id-2",
            "entries": [
                {
                    "account_id": asset.account_id,
                    "direction": "debit",
                    "amount_in_cents": 1_000,
                },
                {
                    "account_id": liability.account_id,
                    "direction": "credit",
                    "amount_in_cents": 900,
                }
            ]
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let error: ErrorResponse = read_json(response).await;
    assert_eq!(error.code, "unbalanced_transaction");
}

#[tokio::test]
#[serial]
async fn get_journal_transactions_transaction_id_returns_transaction() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-3",
        asset.account_id,
        liability.account_id,
        700,
    )
    .await;

    let response = send_request(
        &app,
        Method::GET,
        &format!(
            "/journal/transactions/{}",
            posted.transaction.transaction_id
        ),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let transaction: JournalTransactionResponse = read_json(response).await;
    assert_eq!(
        transaction.transaction_id,
        posted.transaction.transaction_id
    );
    assert_eq!(transaction.reference, "payment-id-3");
    assert_eq!(transaction.description.as_deref(), Some("merchant payout"));
}

#[tokio::test]
#[serial]
async fn post_journal_transactions_transaction_id_reverse_returns_inverse_entries() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-4",
        asset.account_id,
        liability.account_id,
        500,
    )
    .await;

    let response = send_request(
        &app,
        Method::POST,
        &format!(
            "/journal/transactions/{}/reverse",
            posted.transaction.transaction_id
        ),
        Some(json!({
            "reference": "payment-id-4-reversal",
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let reversal: PostTransactionResponse = read_json(response).await;
    assert_eq!(
        reversal.transaction.reversal_of_transaction_id,
        Some(posted.transaction.transaction_id)
    );
    assert_eq!(reversal.transaction.entries.len(), 2);
    assert_eq!(reversal.transaction.entries[0].account_id, asset.account_id);
    assert_eq!(reversal.transaction.entries[0].direction, "credit");
    assert_eq!(
        reversal.transaction.entries[1].account_id,
        liability.account_id
    );
    assert_eq!(reversal.transaction.entries[1].direction, "debit");
    assert_eq!(reversal.transaction.entries[0].amount_in_cents, 500);
}

#[tokio::test]
#[serial]
async fn get_journal_transactions_by_reference_returns_transaction() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-5",
        asset.account_id,
        liability.account_id,
        300,
    )
    .await;

    let response = send_request(
        &app,
        Method::GET,
        "/journal/transactions/by-reference/payment-id-5",
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let transaction: JournalTransactionResponse = read_json(response).await;
    assert_eq!(
        transaction.transaction_id,
        posted.transaction.transaction_id
    );
    assert_eq!(transaction.reference, "payment-id-5");
}

#[tokio::test]
#[serial]
async fn get_accounts_account_id_balance_returns_account_snapshot() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    post_transaction(
        &app,
        "payment-id-6",
        asset.account_id,
        liability.account_id,
        1_250,
    )
    .await;

    let response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}/balance", asset.account_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let balance: BalanceResponse = read_json(response).await;
    assert_eq!(balance.account_id, asset.account_id);
    assert_eq!(balance.currency, "BRL");
    assert_eq!(balance.debits_in_cents, 1_250);
    assert_eq!(balance.credits_in_cents, 0);
    assert_eq!(balance.net_in_cents, 1_250);
}

#[tokio::test]
#[serial]
async fn get_accounts_account_id_statement_supports_cursor_pagination() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    post_transaction(
        &app,
        "payment-id-7",
        asset.account_id,
        liability.account_id,
        200,
    )
    .await;
    post_transaction(
        &app,
        "payment-id-8",
        asset.account_id,
        liability.account_id,
        400,
    )
    .await;

    let first_response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}/statement?limit=1", asset.account_id),
        None,
    )
    .await;

    assert_eq!(first_response.status(), StatusCode::OK);

    let first_page: StatementResponse = read_json(first_response).await;
    assert_eq!(first_page.entries.len(), 1);
    assert_eq!(first_page.entries[0].reference, "payment-id-7");
    assert_eq!(first_page.entries[0].direction, "debit");
    assert_eq!(first_page.entries[0].amount_in_cents, 200);
    assert_eq!(first_page.entries[0].running_balance_in_cents, 200);

    let next_cursor = first_page
        .next_cursor
        .expect("first page should include a continuation cursor");

    let second_response = send_request(
        &app,
        Method::GET,
        &format!(
            "/accounts/{}/statement?limit=1&cursor={}",
            asset.account_id, next_cursor
        ),
        None,
    )
    .await;

    assert_eq!(second_response.status(), StatusCode::OK);

    let second_page: StatementResponse = read_json(second_response).await;
    assert_eq!(second_page.entries.len(), 1);
    assert_eq!(second_page.entries[0].reference, "payment-id-8");
    assert_eq!(second_page.entries[0].running_balance_in_cents, 600);
    assert!(second_page.next_cursor.is_none());
}

#[tokio::test]
#[serial]
async fn get_accounts_account_id_statement_rejects_invalid_cursor() {
    let Some(app) = setup_app().await else {
        return;
    };

    let account = create_account(&app, "cash", "asset").await;
    let response = send_request(
        &app,
        Method::GET,
        &format!(
            "/accounts/{}/statement?cursor=not-base64",
            account.account_id
        ),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let error: ErrorResponse = read_json(response).await;
    assert_eq!(error.code, "invalid_request");
}
