# FieldTrace Rescue & Supply Chain

Offline-first shelter and warehouse management system.

## Stack

- **Backend**: Axum (Rust)
- **Frontend**: Leptos (Rust/WASM, CSR)
- **Database**: SQLite (embedded, single-file)

---

## Test Credentials (quick reference)

The first `/auth/register` call **bootstraps** the initial administrator.
After that, registration is closed and new users must be created by an admin
via `POST /users`.

| Role             | Username    | Password             | Notes                                                                       |
| ---------------- | ----------- | -------------------- | --------------------------------------------------------------------------- |
| Administrator    | `admin`     | `SecurePass12`       | Full access. Create this first with `POST /auth/register`.                  |
| Operations Staff | `staff1`    | `StaffPass1234`      | Create/edit intake, evidence, check-in. No publish/retract, no admin routes. |
| Auditor          | `auditor1`  | `AuditorPass12`      | Read-only everywhere, **plus** publish/retract traceability and report/audit CSV export. |

Bootstrap the admin and log in from the browser at `http://localhost:8080/`,
or with curl:

```bash
# 1. Create the first admin (only works once, before any other user exists)
curl -X POST http://localhost:8080/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"SecurePass12"}'

# 2. Login and keep the session cookie
curl -c /tmp/ck -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"SecurePass12"}'

# 3. Admin creates the other sample users
curl -b /tmp/ck -X POST http://localhost:8080/users \
  -H "Content-Type: application/json" \
  -d '{"username":"staff1","password":"StaffPass1234","role":"operations_staff"}'

curl -b /tmp/ck -X POST http://localhost:8080/users \
  -H "Content-Type: application/json" \
  -d '{"username":"auditor1","password":"AuditorPass12","role":"auditor"}'
```

Valid roles: `administrator`, `operations_staff`, `auditor`.

> Passwords are min 12 chars, Argon2id hashed, and accounts lock after 10
> failed attempts in a 15-minute rolling window (`429 ACCOUNT_LOCKED`).
> Sessions expire after 30 minutes of inactivity (HttpOnly cookie).

---

## Quick Start

```bash
docker compose up --build
```

The application will be available at **http://localhost:8080**.

- API: `http://localhost:8080/health`
- UI: `http://localhost:8080/`

## Running Tests

```bash
chmod +x run_tests.sh
./run_tests.sh
```

`run_tests.sh` is idempotent about stack state:

- If the `w2t52` Docker Compose stack is **already up and healthy**, the
  script reuses the running containers (no rebuild, no restart).
- If the stack is **not running**, it transparently runs
  `docker compose -p w2t52 up -d --build`, waits for `/health`, and then
  runs all test suites.

Either way, the database is reset between suites so each suite starts clean.

Test orchestration (8 steps):

```
[Step 1] Check stack status            (reuse or start)
[Step 2] Wait for /health
[Step 3] Slice 1 tests                 bootstrap + health
[Step 4] Slice 2 tests                 auth + users
[Step 5] Slice 3 tests                 address book
[Step 6] Slice 4 tests                 intake + inspections
[Step 7] Slices 4-11 comprehensive
[Step 8] Remediation suite             audit-report fixes (auditor matrix, idempotency, key rotation, diagnostics, …)
```

Each Docker build also runs `cargo test --release -p fieldtrace-backend`
inside the builder stage — any Rust unit test failure (crypto round-trip,
civil-date formatter, traceability checksum, supply parser, error-envelope
flatten, store-ZIP writer) fails the image build before the runtime image
ships.

## Environment Variables

All environment variables are defined inline in `docker-compose.yml`. No `.env` file required.

| Variable | Default | Description |
|---|---|---|
| PORT | 8080 | Server listen port |
| DATABASE_URL | sqlite:///app/storage/app.db | SQLite database path |
| STATIC_DIR | /app/static | Frontend static files directory |
| STORAGE_DIR | /app/storage | Writable directory for diagnostic ZIPs, uploads |
| RUST_LOG | info | Log level filter |
| ENCRYPTION_KEY | (set in compose) | AES-256 encryption key (64 hex chars) |
| ENCRYPTION_KEY_FILE | (unset) | Optional path to a file holding the hex key. If set, takes precedence over `ENCRYPTION_KEY` and is written to on `/admin/security/rotate-key` |

## Role matrix (enforced server-side)

| Action | administrator | operations_staff | auditor |
|---|---|---|---|
| Create/update intake, inspections, evidence, supply, check-in, members | ✔ | ✔ | ✖ 403 |
| Delete own unlinked evidence | ✔ | only uploader | ✖ 403 |
| **Create traceability code** (`POST /traceability`) | ✔ | ✔ | **✖ 403** |
| Publish / retract traceability | ✔ | ✖ 403 | ✔ |
| Append manual traceability step | ✔ | ✔ | ✖ 403 |
| Create / update transfers | ✔ | ✔ | ✖ 403 |
| Record stock movements | ✔ | ✔ | ✖ 403 |
| CSV reports export / audit log export | ✔ | ✖ 403 | ✔ |
| User management, admin config, diagnostics, key rotation | ✔ | ✖ 403 | ✖ 403 |
| Legal hold toggle on evidence | ✔ | ✖ 403 | ✖ 403 |
| Anti-passback override at `/checkin` | ✔ | ✖ 403 | ✖ 403 |

> **Auditor scope**: read-only across every operational resource, PLUS
> the two explicit write exceptions `POST /traceability/:id/publish` and
> `POST /traceability/:id/retract`. Auditors cannot create traceability
> codes, append manual steps, move transfers, or record stock movements.

## Transfer lifecycle

Transfers are first-class records (`transfers` table) with their own
state machine:

```
queued ──► approved ──► in_transit ──► received
  │           │              │
  └───────────┴──────────────┴────► canceled
```

Any other transition returns `409 CONFLICT`. Endpoints:

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET    | `/transfers` | session | List all transfers (newest first) |
| POST   | `/transfers` | admin/staff | Create in `queued` state |
| GET    | `/transfers/:id` | session | Get a single transfer |
| PATCH  | `/transfers/:id/status` | admin/staff | Update status (state machine enforced) |

The workspace Transfer Queue consumes `/transfers` directly — it no
longer filters intake-status as a shortcut.

## Stock movements and inventory

`inventory_on_hand` is computed from the `stock_movements` append-only
ledger, **not** `COUNT(supply_entries)`. Every receipt, allocation,
adjustment, return, or loss is one signed row:

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET    | `/stock/movements` | session | List movements (filter by `supply_id`, `reason`) |
| POST   | `/stock/movements` | admin/staff | Record one movement |
| GET    | `/stock/inventory` | session | `{ total_on_hand, by_supply }` snapshot |

Sign policy is enforced server-side:

| Reason       | Allowed sign          |
| ------------ | --------------------- |
| `receipt`    | positive only         |
| `return`     | positive only         |
| `allocation` | negative only         |
| `loss`       | negative only         |
| `adjustment` | either (reconciliation) |

Zero deltas and unknown reasons return `400 VALIDATION_ERROR`.

## Dashboard filter set

`/reports/summary` and `/reports/export` honor the same filter keys:

| Query param    | Type       | Matches                                           |
| -------------- | ---------- | ------------------------------------------------- |
| `from`, `to`   | ISO date   | `intake_records.created_at` range                 |
| `status`       | exact      | intake status                                     |
| `intake_type`  | exact      | `animal \| supply \| donation`                    |
| `region`       | exact      | `intake_records.region`                           |
| `tags`         | substring  | CSV match against `intake_records.tags`           |
| `q`            | substring  | full-text across `details`, `region`, `tags`      |

CSV export echoes `filter_region`, `filter_tags`, and `filter_q` so
operators can reproduce any downloaded report.

## Traceability timeline

Every traceability code has an append-only `traceability_steps`
timeline. Steps are written automatically on:

- `create` — code generated
- `publish` — version bumped + comment
- `retract` — version bumped + comment
- `inspection` — when a linked inspection resolves

Plus manual operator notes via `POST /traceability/:id/steps` (admin/staff only).

Retrieve via `GET /traceability/:id/steps` — returns the ordered list
as `TraceStepResponse` rows.

### Account deletion (cooling-off + FK-safe anonymization)

- `POST /account/delete` schedules deletion in 7 days. You can still log in
  during the cooling-off window.
- `POST /account/cancel-deletion` clears the pending request.
- **Anonymization, not hard delete.** When the 7-day window lapses, the
  background job `account_deletion_purge` (hourly) — or the admin endpoint
  `POST /admin/account-purge` (immediate, for GDPR response) — transactionally
  strips the user's personal data:
  1. Every `address_book` row for the user is deleted (personal data).
  2. Every active session for the user is dropped.
  3. `checkin_ledger.override_by` and `audit_logs.actor_id` references are
     set to `NULL`.
  4. The `users` row itself is rewritten in place: `username` rotated to
     `anon-<uuid>`, `password_hash` replaced with `$invalid$anonymized$`,
     `anonymized = 1`, `deletion_requested_at = NULL`.
  - Because the row still exists, every `NOT NULL` FK in `intake_records`,
    `inspections`, `evidence_records`, `supply_entries`, `traceability_codes`,
    `traceability_events`, and `config_versions` remains valid — the history
    is preserved for audit, but no longer traceable to a natural person.
  - Login, register, and admin user listing all filter `WHERE anonymized = 0`
    so the tombstone is invisible to every public-facing code path.
- Operator immediate purge: `POST /admin/account-purge` with
  `{"grace_period_days": N}` (default `7`). Admin only. Runs the same
  transactional logic and returns the number of users anonymized.

### Idempotency

Every mutating route (`POST`/`PATCH`/`PUT`/`DELETE`) on the protected router
accepts an optional `Idempotency-Key` header. Scope is
`method + normalized_route + actor_id + key`; the window is 10 minutes.
Retries within the window return the original response body/status and a
`Idempotent-Replay: true` header.

### Encryption key rotation

`POST /admin/security/rotate-key` with `{"new_key_hex": "<64 hex chars>"}`
decrypts every encrypted-at-rest field, re-encrypts with the new key, and
commits in a single SQLite transaction. The in-memory cipher is replaced
only after commit. If `ENCRYPTION_KEY_FILE` is configured, the new key is
atomically written there as well.

### Evidence retention (365 days)

Evidence records are kept forever by default unless they satisfy all three
conditions:

1. `created_at` is older than the retention window (default 365 days).
2. `linked = 0` — the row is not attached to any intake, inspection,
   traceability code, or check-in event.
3. `legal_hold = 0` — the admin has not placed the row on legal hold.

Enforcement happens in two places:

- **Background job** `evidence_retention` runs hourly and calls
  `jobs::run_evidence_retention(db, 365)`. Deletions happen in a single
  SQLite transaction that re-checks the `linked` and `legal_hold` flags
  inside the tx to avoid races with concurrent link/hold flips. A
  `structured_logs` row is emitted per run showing the delete count.
- **Admin endpoint** `POST /admin/retention-purge` with
  `{"max_age_days": N}` (default 365, min 0) runs the same logic
  immediately — integration tests use `max_age_days: 0` to drive a
  deterministic same-second sweep.

### Local media compression

Every `POST /media/upload/complete` runs the payload size through
`apply_compression_policy(media_type, original_bytes)` before insert.
The policy is deterministic per media type:

| Media type | Target ratio | Floor (skip below) |
| ---------- | ------------ | ------------------- |
| `photo`    | 0.70         | 256 KiB             |
| `video`    | 0.60         | 2 MiB               |
| `audio`    | 0.50         | 128 KiB             |
| other      | 1.00 (skip)  | —                   |

Files at or below the per-type floor pass through unchanged
(`compression_applied = false`). Above the floor the row is persisted with
`compressed_bytes`, `compression_ratio`, `compression_applied` populated,
and `EvidenceResponse` carries the same three fields back to the client.

A guard rejects any compression result that produced a `compressed_bytes >
size_bytes` to prevent an upstream policy bug from letting oversized
"compressed" payloads through.

### Draft autosave and session-expiry restore

Intake and address-book forms autosave their inputs to `localStorage` on
every change. The keys live under the `fieldtrace.draft.<form_id>` prefix
defined in `fieldtrace-shared`.

On any 401 response the API client:

1. Calls `draft::flash_session_expired()` which stores a user-visible
   banner message.
2. Calls `draft::preserve_route(current_path)` so the app shell can land
   the user back on the same page after re-login.

On app mount, `draft::consume_session_flash()` surfaces the banner and
each form's `load_draft(form_id)` re-seeds the signals from localStorage.
`clear_draft(form_id)` runs on successful submit so the saved state does
not linger after a successful post.

### Diagnostic package

`POST /admin/diagnostics/export` builds a real ZIP (PKZIP "stored" method,
no external dependencies) under `/app/storage/diagnostics/{id}.zip` and
returns `{download_id, download_url, size_bytes, expires_in_seconds}`.

The archive contains **four files**:

| File                  | Content                                                                                                             |
| --------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `logs.txt`            | Up to 5000 most-recent `structured_logs` rows from the last 7 days, one per line, with sensitive markers sanitized  |
| `metrics.json`        | Up to 1000 most-recent `job_metrics` rows (job name, status, run count, last error, last run time)                  |
| `config_history.json` | Every `config_versions` row with its **full snapshot payload** (not just metadata) so operators can diff or recover |
| `audit_summary.csv`   | Aggregated `audit_logs` counts by action, sensitive payloads omitted                                                |

Files older than 1 hour are removed by the `diagnostics_cleanup` background
job. Download via `GET /admin/diagnostics/download/{id}`.

### Structured logs

Key backend operations (`intake.create`, `intake.status_update`,
`inspection.resolve`, `evidence.upload_complete`, `supply.create`,
`traceability.publish`, `traceability.retract`, and failed login attempts)
write a row into the `structured_logs` table via `common::slog`. These
rows are what the diagnostic ZIP's `logs.txt` is built from.

The writer passes every message through `sanitize_log_message` which
refuses to store anything containing known-sensitive markers (`password`,
`$argon2`, `Bearer `, `Authorization:`, `token=`, `api_key=`, `session_id=`,
`secret=`). Tampered messages are replaced with `[REDACTED: sensitive
content blocked]` and capped to 2000 characters.

Admin can inspect the log stream via `GET /admin/logs` (last 200 rows).

## Slices Implemented

| Slice | Feature |
|---|---|
| 1 | Foundation — health, trace IDs, SQLite WAL, Docker, static frontend |
| 2 | Auth + Users — Argon2id, lockout, sessions, RBAC |
| 3 | Address Book — AES-256-GCM encryption, ZIP+4 validation, phone masking, object-level auth |
| 4 | Intake + Inspections — state machine, 409 on invalid transitions |
| 5 | Evidence + Chunked Upload — size limits, EXIF flagging, link immutability, legal hold |
| 6 | Supply Parsing — deterministic color/size normalization, `needs_review` state |
| 7 | Traceability — Luhn checksum codes, publish/retract (Admin/Auditor only), offline verify |
| 8 | Check-In — anti-passback 2-min per facility, admin-only override |
| 9 | Dashboard + Reports — metrics summary, CSV export (Admin/Auditor only) |
| 10 | Admin Config — versioning, rollback, diagnostic export, jobs metrics |
| 11 | Security + Audit — append-only audit log, CSV export with [REDACTED] masking |
| 12 | Final Polish — integrated UI, test orchestration |

## Test Summary

The orchestrator runs 9 suites:

| Step | Suite              | Coverage                                                                 |
| ---- | ------------------ | ------------------------------------------------------------------------ |
| 3    | S1-Unit / S1-API   | Bootstrap + health + trace-id + static assets                            |
| 4    | S2-Auth suites     | Registration bootstrap guard, login, lockout, session expiry             |
| 5    | S3-AddrBook        | CRUD + encryption round-trip + object-level auth                         |
| 6    | S4-Intake          | State machine transitions + inspection resolve-once                      |
| 7    | S4-11-Full         | Cross-slice comprehensive (evidence, supply, traceability, checkin, dashboard, admin, audit) |
| 8    | Remediation        | Audit-report fixes (auditor matrix, object-level auth, idempotency, key rotation, diagnostics) |
| 9    | **Blockers**       | Final acceptance: address-book auditor lockout, FK-safe purge, config cap + rollback, diagnostic snapshot content, structured_logs writes, sensitive-leak prevention |

Rust unit tests run during `docker compose build` (cargo test --release):
civil-date formatter, AES round-trip + tamper, error envelope flatten,
ZIP writer + CRC32, traceability checksum, supply parser, log sanitizer.
