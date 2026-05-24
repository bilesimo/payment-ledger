use axum::http::{Method, StatusCode};
use payment_ledger::modules::accounts::dto::AccountResponse;
use serial_test::serial;

use crate::support::{ErrorResponse, read_json, send_request, setup_app};

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

#[tokio::test]
#[serial]
async fn creates_account() {
    let Some(app) = setup_app().await else {
        return;
    };

    let account = create_account(&app, "cash", "asset").await;

    assert!(account.account_id.value() > 0);
    assert_eq!(account.name.as_deref(), Some("cash"));
    assert_eq!(account.account_type.as_db_value(), "asset");
    assert_eq!(account.currency.as_str(), "BRL");
}

#[tokio::test]
#[serial]
async fn gets_account_by_id() {
    let Some(app) = setup_app().await else {
        return;
    };

    let created = create_account(&app, "merchant payable", "liability").await;
    let response = send_request(
        &app,
        Method::GET,
        &format!("/accounts/{}", created.account_id.value()),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let account: AccountResponse = read_json(response).await;
    assert_eq!(account.account_id, created.account_id);
    assert_eq!(account.name.as_deref(), Some("merchant payable"));
    assert_eq!(account.account_type.as_db_value(), "liability");
}

#[tokio::test]
#[serial]
async fn returns_not_found_for_unknown_account() {
    let Some(app) = setup_app().await else {
        return;
    };

    let response = send_request(&app, Method::GET, "/accounts/999999", None).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let error: ErrorResponse = read_json(response).await;
    assert_eq!(error.code, "account_not_found");
    assert!(error.message.contains("999999"));
}
