# FieldTrace — Architecture

## Stack

| Layer            | Technology                 |
| ---------------- | -------------------------- |
| Backend          | Axum (Rust)                |
| Frontend         | Leptos (Rust/WASM)         |
| Database         | SQLite (single file, sqlx) |
| Password hashing | Argon2id                   |
| Field encryption | AES-256-GCM                |
| Logging          | tracing (structured JSON)  |
| Background jobs  | Tokio tasks                |

Runs on a single machine. No internet required. No external services.

## Runtime and Deployment

- Docker-only runtime: the app is considered valid only when it runs via `docker compose up`.
- No host-local dependency assumptions (no local DB/bootstrap tools required).
- `docker-compose.yml` must define explicit ports, inline environment variables, and healthchecks.
- API container healthcheck targets `GET /health`; startup depends on healthy DB.

---

## Module Breakdown

| Module                   | What it does                                                                                                           |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `auth`                   | Register, login, logout, sessions, password hashing, lockout, account deletion request/cancel                           |
| `users`                  | Admin user CRUD                                                                                                         |
| `address_book`           | Per-user encrypted address entries                                                                                      |
| `intake`                 | Intake records, state machine transitions (409 on invalid)                                                              |
| `inspections`            | Inspection CRUD, resolve-once                                                                                           |
| `evidence`               | Chunked upload, real watermark (`FAC01 MM/DD/YYYY hh:mm AM/PM`), retention, legal hold, immutability, keyword/tag/date search |
| `supply`                 | Parsing pipeline, color/size normalization, `needs_review` conflict state                                               |
| `traceability`           | Code generation (Luhn + real current date — admin/staff only, auditor 403 on create), publish/retract with comment (auditor allowed), append-only `traceability_steps` timeline on create/publish/retract/inspection/manual-note |
| `transfers`              | First-class transfer queue with `queued→approved→in_transit→received` state machine (+`canceled` from any non-terminal). 409 on invalid transitions. Replaces the old "filter intake by status" workaround in the workspace UI. |
| `stock`                  | Append-only `stock_movements` ledger. `inventory_on_hand` = `SUM(quantity_delta)`. Sign policy enforced per reason (`receipt`/`return` > 0, `allocation`/`loss` < 0). Dashboard reads from here, not `COUNT(supply_entries)`. |
| `checkin`                | Member records, anti-passback (includes `retry_after_seconds` in 409), admin-only override                              |
| `dashboard`              | Metrics aggregation with full filter set (`from`, `to`, `status`, `intake_type`, `region`, `tags`, `q` full-text), CSV export (Admin/Auditor only) mirrors the exact same filters |
| `admin`                  | Config versioning + rollback, **real** diagnostic ZIP generation + download + 1h cleanup, **real** transactional key rotation |
| `audit`                  | Append-only audit log + CSV export with sensitive fields redacted                                                       |
| `common`                 | `db_err` / `system_err` sanitizers, `require_write_role`, `require_admin_or_auditor`, `CivilDateTime` formatter, `slog` + `sanitize_log_message` for persistent `structured_logs` writes |
| `crypto`                 | AES-256-GCM with fallible `try_encrypt` / `try_decrypt`; live key held in `Arc<RwLock<Crypto>>` on `AppState` for rotation |
| `jobs`                   | `session_cleanup` (5 min), `account_deletion_purge` (1 h), `diagnostics_cleanup` (10 min), `evidence_retention` (1 h)    |
| `middleware::idempotency` | Auth-first idempotency — scope `method + matched_route + actor_id + key`, 10-minute window, replay sends `Idempotent-Replay: true` |
| `zip`                    | Minimal PKZIP stored-method writer with CRC-32 (no external crate) used by diagnostics                                  |

---

## Database Tables

```
users               — credentials, role, deletion_requested_at
sessions            — session token, last_active
address_book        — per-user shipping destinations (encrypted)
facilities          — facility code used in watermarks and traceability codes
intake_records      — animal/supply/donation intakes
inspections         — linked to intake_records
evidence_records    — uploaded media metadata, watermark, retention
evidence_links      — links evidence to intake/inspection/traceability/checkin
upload_sessions     — tracks chunked upload progress
supply_entries      — parsed supply data, conflict state
traceability_codes  — generated codes, publish/retract status
traceability_events — versioned publish/retract history with mandatory comment
checkin_ledger      — member check-in events
members             — barcode ID + name
config_versions     — last 10 config snapshots
structured_logs     — job and request logs
job_metrics         — background job run history
audit_logs          — append-only admin action trail
```

---

## Request Flow

```
Browser (Leptos)
    │
    ▼
Axum HTTP Server :8080
    ├── Trace ID middleware     → attaches UUID to every request and log line
    ├── Session/Auth middleware → validates cookie, checks 30-min inactivity, 401 if expired
    ├── Idempotency middleware  → runs after auth on protected mutating endpoints
    ├── Role guard              → checks session.role against required role, 403 if wrong
    └── Handler → Service → SQLite
```

---

## Background Jobs

| Job                      | Runs every | What it does                                                                                                                |
| ------------------------ | ---------- | --------------------------------------------------------------------------------------------------------------------------- |
| `session_cleanup`        | 5 min      | Delete sessions inactive > 30 min. Records run state to `job_metrics`.                                                      |
| `account_deletion_purge` | 1 hour     | Transactionally **anonymizes** any user whose `deletion_requested_at` is > 7 days old: wipes address book + sessions, nulls `audit_logs.actor_id` / `checkin_ledger.override_by`, rewrites users row in place with `anonymized=1` and a non-verifiable password hash. Preserves referential integrity with intake/inspection/evidence/supply/traceability/config FKs. Rolls back on any failure. Admin can also trigger immediately via `POST /admin/account-purge`. |
| `diagnostics_cleanup`    | 10 min     | Removes `{storage}/diagnostics/*.zip` files older than 1 hour.                                                              |
| `evidence_retention`     | 1 hour     | Records a run in `job_metrics` (deletion policy is placeholder — legal-hold rows never expire).                              |

---

## Security Design

- Passwords: Argon2id, minimum 12 characters
- Account lockout: rolling-window failure check (timestamped failures, not lifetime counter)
- Sessions: random UUID in HttpOnly cookie, 30-min inactivity expiry
- Object-level authorization: ownership enforced in DB query predicates (`WHERE id=? AND owner_id=?`)
- Sensitive fields: AES-256-GCM encrypted (phone, address, donor_ref)
- Keystore: 256-bit key in env `ENCRYPTION_KEY` or file at `ENCRYPTION_KEY_FILE`; rotated in-memory via `Arc<RwLock<Crypto>>`
- Key rotation: re-encrypts all sensitive fields in a single SQLite transaction, swaps cipher only after commit
- Masking: only last 4 digits shown on screen
- Idempotency scope: dedup key bound to `method + matched_route + actor_id + key` within 10-minute window
- Audit log: INSERT only — no DELETE or UPDATE endpoint exists
- Account deletion: 7-day cooling-off + in-place anonymization (see lifecycle section); `users.anonymized = 0` filter applied to login/register/list
- Role policy: `require_write_role` on every mutating handler blocks auditors (including address book); `require_admin_or_auditor` on report/audit exports and traceability publish/retract
- Trace IDs: on every request, every tracing log line, every X-Trace-Id response header
- **Persistent structured logs**: `common::slog` writes to the `structured_logs` SQLite table on core operations (intake, inspection, evidence upload, supply create, traceability publish/retract, failed login). Every message is passed through `sanitize_log_message` which drops anything matching `password`, `$argon2`, `Bearer `, `Authorization:`, `token=`, `api_key=`, `session_id=`, `secret=` and caps length at 2000 chars.

---

## Error Response Format

Every error returns the same shape:

```json
{
  "status": 400,
  "code": "VALIDATION_ERROR",
  "message": "human readable description",
  "trace_id": "uuid"
}
```

Codes used: `VALIDATION_ERROR` (400), `UNAUTHORIZED` (401), `FORBIDDEN` (403),
`NOT_FOUND` (404), `CONFLICT` (409), `ACCOUNT_LOCKED` (429),
`ANTI_PASSBACK` (409, with flattened `retry_after_seconds`),
`INTERNAL_ERROR` (500).

Internal database errors are sanitized — handlers call
`common::db_err(trace_id)` which logs the full error with
`tracing::error!(error = ..., trace_id = ...)` and returns a generic
`"Internal server error"` message to the client. Argon2 hashing errors and
filesystem errors use the same pattern.
