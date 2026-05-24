use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use payment_ledger::ui;
use tower::ServiceExt;

#[tokio::test]
async fn serves_embedded_ui_at_root() {
    let response = ui::router()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should be readable");
    let html = String::from_utf8(body.to_vec()).expect("ui should be utf-8");

    assert!(html.contains("Payment Ledger Console"));
    assert!(html.contains("create-account-form"));
    assert!(html.contains("/journal/transactions"));
}
