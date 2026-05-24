use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    AppState,
    modules::accounts::{
        dto::{AccountResponse, CreateAccountRequest},
        mapper,
    },
    shared::errors::AppError,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounts", post(create_account))
        .route("/accounts/:account_id", get(get_account))
}

async fn create_account(
    State(state): State<AppState>,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AppError> {
    let account = state
        .account_service
        .create_account(mapper::to_create_account(request))
        .await?;

    Ok((StatusCode::CREATED, Json(mapper::to_response(account))))
}

async fn get_account(
    State(state): State<AppState>,
    Path(account_id): Path<i64>,
) -> Result<Json<AccountResponse>, AppError> {
    let account = state.account_service.get_account(account_id.into()).await?;

    Ok(Json(mapper::to_response(account)))
}
