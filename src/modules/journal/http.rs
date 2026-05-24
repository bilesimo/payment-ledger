use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    AppState,
    modules::journal::{
        dto::{
            BalanceResponse, CreateTransactionRequest, JournalTransactionResponse,
            PostTransactionResponse, ReverseTransactionRequest, StatementQueryParams,
            StatementResponse,
        },
        mapper,
    },
    shared::errors::AppError,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/journal/transactions", post(post_transaction))
        .route(
            "/journal/transactions/:transaction_id",
            get(get_transaction),
        )
        .route(
            "/journal/transactions/:transaction_id/reverse",
            post(reverse_transaction),
        )
        .route(
            "/journal/transactions/by-reference/:reference",
            get(get_transaction_by_reference),
        )
        .route("/accounts/:account_id/balance", get(get_balance))
        .route("/accounts/:account_id/statement", get(get_statement))
}

async fn post_transaction(
    State(state): State<AppState>,
    Json(request): Json<CreateTransactionRequest>,
) -> Result<(StatusCode, Json<PostTransactionResponse>), AppError> {
    let result = state
        .journal_service
        .post_transaction(mapper::to_post_transaction(request)?)
        .await?;

    let status = if result.was_replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(mapper::to_post_response(result.transaction))))
}

async fn reverse_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<i64>,
    Json(request): Json<ReverseTransactionRequest>,
) -> Result<(StatusCode, Json<PostTransactionResponse>), AppError> {
    let result = state
        .journal_service
        .reverse_transaction(
            transaction_id.into(),
            request.reference,
            request.description,
        )
        .await?;

    let status = if result.was_replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(mapper::to_post_response(result.transaction))))
}

async fn get_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<i64>,
) -> Result<Json<JournalTransactionResponse>, AppError> {
    let transaction = state
        .journal_service
        .get_transaction(transaction_id.into())
        .await?;

    Ok(Json(mapper::to_transaction_response(transaction)))
}

async fn get_transaction_by_reference(
    State(state): State<AppState>,
    Path(reference): Path<String>,
) -> Result<Json<JournalTransactionResponse>, AppError> {
    let transaction = state
        .journal_service
        .get_transaction_by_reference(reference)
        .await?;

    Ok(Json(mapper::to_transaction_response(transaction)))
}

async fn get_balance(
    State(state): State<AppState>,
    Path(account_id): Path<i64>,
) -> Result<Json<BalanceResponse>, AppError> {
    let balance = state.journal_service.get_balance(account_id.into()).await?;

    Ok(Json(mapper::to_balance_response(balance)))
}

async fn get_statement(
    State(state): State<AppState>,
    Path(account_id): Path<i64>,
    Query(query): Query<StatementQueryParams>,
) -> Result<Json<StatementResponse>, AppError> {
    let statement = state
        .journal_service
        .get_statement(
            account_id.into(),
            query.cursor,
            query.from,
            query.to,
            query.limit,
        )
        .await?;

    Ok(Json(mapper::to_statement_response(statement)))
}
