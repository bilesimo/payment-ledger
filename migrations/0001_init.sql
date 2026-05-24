create type account_type as enum (
  'asset',
  'liability',
  'revenue',
  'expense'
);

create type entry_direction as enum (
  'debit',
  'credit'
);

create table accounts (
  id bigint primary key,
  name text null,
  account_type account_type not null,
  currency char(3) not null default 'BRL',
  created_at timestamptz not null,
  constraint accounts_currency_brl check (currency = 'BRL')
);

create table journal_transactions (
  id bigint primary key,
  reference text not null unique,
  payload_fingerprint text not null,
  description text null,
  reversal_of_transaction_id bigint null references journal_transactions(id),
  created_at timestamptz not null
);

create unique index journal_transactions_single_reversal_idx
  on journal_transactions (reversal_of_transaction_id)
  where reversal_of_transaction_id is not null;

create table journal_entries (
  id bigint primary key,
  transaction_id bigint not null references journal_transactions(id) on delete restrict,
  account_id bigint not null references accounts(id) on delete restrict,
  direction entry_direction not null,
  amount_in_cents bigint not null,
  created_at timestamptz not null,
  constraint journal_entries_amount_positive check (amount_in_cents > 0)
);

create index journal_entries_transaction_idx
  on journal_entries (transaction_id);

create index journal_entries_account_statement_idx
  on journal_entries (account_id, created_at, transaction_id, id);
