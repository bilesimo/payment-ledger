# Payment Ledger

`payment-ledger` is a Rust HTTP service for a small double-entry ledger. It exposes APIs to create accounts, post balanced journal transactions, reverse transactions, query balances, and page through account statements.

The service is built with `axum`, persists data in PostgreSQL through `sqlx`, and runs database migrations automatically on startup.

## What It Does

- Creates ledger accounts with typed classifications: `asset`, `liability`, `revenue`, `expense`
- Posts balanced journal transactions with idempotency by `reference`
- Reverses an existing transaction by creating equal and opposite entries
- Returns account balances using account-type-aware net balance rules
- Returns account statements with cursor pagination
- Uses BRL minor units (`amount_in_cents`) for all monetary values

## Tech Stack

- Rust 2024 edition
- Axum
- Tokio
- SQLx
- PostgreSQL 16
- Docker Compose for local Postgres

## Project Layout

```text
.
├── migrations/             # SQL schema migrations
├── src/
│   ├── infra/              # config and database wiring
│   ├── modules/
│   │   ├── accounts/       # account domain, service, HTTP handlers
│   │   └── journal/        # journal domain, service, HTTP handlers
│   ├── shared/             # ids, money, shared errors
│   ├── lib.rs              # app assembly entry point
│   └── main.rs             # executable entry point
└── docker-compose.yml      # local PostgreSQL instance
```

## Requirements

- Rust toolchain installed
- Docker and Docker Compose

If you already have PostgreSQL running locally, you can use that instead of Docker as long as `DATABASE_URL` points to it.

## Getting Started

1. Copy the example environment file:

```bash
cp .env.example .env
```

2. Start PostgreSQL:

```bash
docker compose up -d
```

3. Run the service:

```bash
cargo run
```

By default, the API listens on `127.0.0.1:3000`.

The service also serves a small browser UI at [http://127.0.0.1:3000/](http://127.0.0.1:3000/) for creating accounts, posting transactions, reversing transactions, and inspecting balances or statements without using `curl`.

Migrations in [`migrations/0001_init.sql`](/Users/bilesimo/Development/payment-ledger/migrations/0001_init.sql) are applied automatically when the app starts.

## Configuration

The service reads configuration from environment variables.

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | none | PostgreSQL connection string |
| `NODE_ID` | Yes | none | Snowflake node identifier used when generating IDs |
| `HTTP_ADDR` | No | `127.0.0.1:3000` | Socket address for the HTTP server |

Example:

```env
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/payment_ledger
NODE_ID=1
HTTP_ADDR=127.0.0.1:3000
```

## API Overview

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/accounts` | Create an account |
| `GET` | `/accounts/:account_id` | Fetch an account |
| `POST` | `/journal/transactions` | Post a balanced transaction |
| `GET` | `/journal/transactions/:transaction_id` | Fetch a transaction by ID |
| `GET` | `/journal/transactions/by-reference/:reference` | Fetch a transaction by reference |
| `POST` | `/journal/transactions/:transaction_id/reverse` | Reverse a transaction |
| `GET` | `/accounts/:account_id/balance` | Fetch balance totals for an account |
| `GET` | `/accounts/:account_id/statement` | Fetch a paginated account statement |

## Example Workflow

Create an asset account:

```bash
curl -X POST http://127.0.0.1:3000/accounts \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "Cash",
    "account_type": "asset"
  }'
```

Create a revenue account:

```bash
curl -X POST http://127.0.0.1:3000/accounts \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "Sales",
    "account_type": "revenue"
  }'
```

Post a balanced transaction:

```bash
curl -X POST http://127.0.0.1:3000/journal/transactions \
  -H 'Content-Type: application/json' \
  -d '{
    "reference": "sale-1001",
    "description": "Product sale",
    "entries": [
      {
        "account_id": <asset_account_id>,
        "direction": "debit",
        "amount_in_cents": 10000
      },
      {
        "account_id": <revenue_account_id>,
        "direction": "credit",
        "amount_in_cents": 10000
      }
    ]
  }'
```

Fetch the balance of the asset account:

```bash
curl http://127.0.0.1:3000/accounts/<asset_account_id>/balance
```

Fetch the statement of the asset account:

```bash
curl 'http://127.0.0.1:3000/accounts/<asset_account_id>/statement?limit=50'
```

Reverse a transaction:

```bash
curl -X POST http://127.0.0.1:3000/journal/transactions/<transaction_id>/reverse \
  -H 'Content-Type: application/json' \
  -d '{
    "reference": "sale-1001-reversal",
    "description": "Customer refund"
  }'
```

## Domain Rules

- A journal transaction must contain at least two entries.
- Total debits must equal total credits.
- Entry amounts must be positive.
- References are trimmed and must not be empty.
- Reposting the same `reference` with the same payload returns the original transaction instead of creating a duplicate.
- Reusing a `reference` with a different payload returns `409 Conflict`.
- A transaction can only be reversed once.
- Reversing a reversal is not allowed in v1.
- Account names are trimmed; blank names become `null`.
- Currency is fixed to `BRL` in the current schema.

## Pagination

`GET /accounts/:account_id/statement` supports:

- `limit`
- `from`
- `to`
- `cursor`

The response includes `next_cursor` when more rows are available. Pass that cursor back on the next request to continue from the last page.

## Error Format

Errors are returned as JSON:

```json
{
  "code": "invalid_request",
  "message": "reference must be present"
}
```

Common statuses:

- `422 Unprocessable Entity` for validation errors
- `404 Not Found` for missing accounts or transactions
- `409 Conflict` for idempotency conflicts or invalid reversal attempts
- `500 Internal Server Error` for unexpected failures

## Running Tests

```bash
cargo test
```

At the time this README was generated, the current test suite passes locally.
