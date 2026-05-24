use axum::http::{Method, StatusCode};
use payment_ledger::modules::accounts::dto::AccountResponse;
use serial_test::serial;

use crate::support::{ErrorResponse, read_json, send_request, setup_app};

#[derive(serde::Deserialize)]
struct PostTransactionResponse {
    transaction: JournalTransactionResponse,
}

#[derive(serde::Deserialize)]
struct JournalTransactionResponse {
    transaction_id: i64,
    reference: String,
    description: Option<String>,
    reversal_of_transaction_id: Option<i64>,
    entries: Vec<JournalEntryResponse>,
}

#[derive(serde::Deserialize)]
struct JournalEntryResponse {
    account_id: i64,
    direction: String,
    amount_in_cents: i64,
}

#[derive(serde::Deserialize)]
struct BalanceResponse {
    account_id: i64,
    currency: String,
    debits_in_cents: i64,
    credits_in_cents: i64,
    net_in_cents: i64,
}

#[derive(serde::Deserialize)]
struct StatementResponse {
    entries: Vec<StatementEntryResponse>,
    next_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
struct StatementEntryResponse {
    reference: String,
    direction: String,
    amount_in_cents: i64,
    running_balance_in_cents: i64,
}

async fn create_account(app: &axum::Router, name: &str, account_type: &str) -> AccountResponse {
    let response = send_request(
        app,
        Method::POST,
        "/accounts",
        Some(serde_json::json!({
            "name": name,
            "account_type": account_type,
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    read_json(response).await
}

async fn post_transaction(
    app: &axum::Router,
    reference: &str,
    debit_account_id: i64,
    credit_account_id: i64,
    amount_in_cents: i64,
) -> (StatusCode, PostTransactionResponse) {
    let response = send_request(
        app,
        Method::POST,
        "/journal/transactions",
        Some(serde_json::json!({
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
async fn posts_transaction_idempotently() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;

    let (first_status, first) = post_transaction(
        &app,
        "payment-id-1",
        asset.account_id.value(),
        liability.account_id.value(),
        1_000,
    )
    .await;
    let (second_status, second) = post_transaction(
        &app,
        "payment-id-1",
        asset.account_id.value(),
        liability.account_id.value(),
        1_000,
    )
    .await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first.transaction.transaction_id, second.transaction.transaction_id);
}

#[tokio::test]
#[serial]
async fn rejects_unbalanced_transaction_payload() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;

    let response = send_request(
        &app,
        Method::POST,
        "/journal/transactions",
        Some(serde_json::json!({
            "reference": "payment-id-2",
            "entries": [
                {
                    "account_id": asset.account_id.value(),
                    "direction": "debit",
                    "amount_in_cents": 1_000,
                },
                {
                    "account_id": liability.account_id.value(),
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
async fn gets_transaction_by_id() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-3",
        asset.account_id.value(),
        liability.account_id.value(),
        700,
    )
    .await;

    let response = send_request(
        &app,
        Method::GET,
        &format!("/journal/transactions/{}", posted.transaction.transaction_id),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let transaction: JournalTransactionResponse = read_json(response).await;
    assert_eq!(transaction.transaction_id, posted.transaction.transaction_id);
    assert_eq!(transaction.reference, "payment-id-3");
    assert_eq!(transaction.description.as_deref(), Some("merchant payout"));
}

#[tokio::test]
#[serial]
async fn reverses_transaction() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-4",
        asset.account_id.value(),
        liability.account_id.value(),
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
        Some(serde_json::json!({
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
    assert_eq!(reversal.transaction.entries[0].account_id, asset.account_id.value());
    assert_eq!(reversal.transaction.entries[0].direction, "credit");
    assert_eq!(reversal.transaction.entries[1].account_id, liability.account_id.value());
    assert_eq!(reversal.transaction.entries[1].direction, "debit");
    assert_eq!(reversal.transaction.entries[0].amount_in_cents, 500);
}

#[tokio::test]
#[serial]
async fn gets_transaction_by_reference() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    let (_, posted) = post_transaction(
        &app,
        "payment-id-5",
        asset.account_id.value(),
        liability.account_id.value(),
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
    assert_eq!(transaction.transaction_id, posted.transaction.transaction_id);
    assert_eq!(transaction.reference, "payment-id-5");
}

#[tokio::test]
#[serial]
async fn gets_account_balance() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    post_transaction(
        &app,
        "payment-id-6",
        asset.account_id.value(),
        liability.account_id.value(),
        1_250,
    )
    .await;

    let response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}/balance", asset.account_id.value()),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let balance: BalanceResponse = read_json(response).await;
    assert_eq!(balance.account_id, asset.account_id.value());
    assert_eq!(balance.currency, "BRL");
    assert_eq!(balance.debits_in_cents, 1_250);
    assert_eq!(balance.credits_in_cents, 0);
    assert_eq!(balance.net_in_cents, 1_250);
}

#[tokio::test]
#[serial]
async fn paginates_statement() {
    let Some(app) = setup_app().await else {
        return;
    };

    let asset = create_account(&app, "cash", "asset").await;
    let liability = create_account(&app, "merchant payable", "liability").await;
    post_transaction(
        &app,
        "payment-id-7",
        asset.account_id.value(),
        liability.account_id.value(),
        200,
    )
    .await;
    post_transaction(
        &app,
        "payment-id-8",
        asset.account_id.value(),
        liability.account_id.value(),
        400,
    )
    .await;

    let first_response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}/statement?limit=1", asset.account_id.value()),
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
            asset.account_id.value(),
            next_cursor
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
async fn rejects_invalid_statement_cursor() {
    let Some(app) = setup_app().await else {
        return;
    };

    let account = create_account(&app, "cash", "asset").await;
    let response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}/statement?cursor=not-base64", account.account_id.value()),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let error: ErrorResponse = read_json(response).await;
    assert_eq!(error.code, "invalid_request");
}
