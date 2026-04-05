# FieldTrace — API Reference

Base URL: `http://localhost:8080`  
Auth: session cookie (HttpOnly) set on login. All endpoints require it unless marked public.

Security notes:
- Protected routes evaluate auth/session before idempotency replay logic.
- Object-level authorization is enforced server-side for user-owned resources.

---

## Health

| Method | Path    | Auth   | Description               |
| ------ | ------- | ------ | ------------------------- |
| GET    | /health | public | Returns `{"status":"ok"}` |

---

## Auth

| Method | Path                  | Auth    | Description                        |
| ------ | --------------------- | ------- | ---------------------------------- |
| POST   | /auth/register        | public  | Bootstrap first Administrator only |
| POST   | /auth/login           | public  | Login, sets session cookie         |
| POST   | /auth/logout          | session | Invalidates session                |
| GET    | /auth/me              | session | Current user info                  |
| PATCH  | /auth/change-password | session | Change own password (min 12 chars) |

Lockout policy (implementation target): 10 failed login attempts within 15 minutes lock the account for 15 minutes.
Registration policy: `POST /auth/register` is allowed only before first account bootstrap; later account creation is admin-only via `POST /users`.

---

## Users

| Method | Path                     | Auth    | Description                                  |
| ------ | ------------------------ | ------- | -------------------------------------------- |
| GET    | /users                   | admin   | List all users                               |
| POST   | /users                   | admin   | Create user                                  |
| PATCH  | /users/:id               | admin   | Update user role or status                   |
| DELETE | /users/:id               | admin   | Delete user                                  |
| POST   | /account/delete          | session | Request account deletion (7-day cooling-off) |
| POST   | /account/cancel-deletion | session | Cancel pending deletion                      |

---

## Address Book

| Method | Path              | Auth    | Description                                     |
| ------ | ----------------- | ------- | ----------------------------------------------- |
| GET    | /address-book     | session | List own entries                                |
| POST   | /address-book     | session | Create entry (ZIP+4 validated, phone encrypted) |
| PATCH  | /address-book/:id | session | Update own entry only                           |
| DELETE | /address-book/:id | session | Delete own entry only                           |

---

## Intake Records

| Method | Path               | Auth         | Description          |
| ------ | ------------------ | ------------ | -------------------- |
| GET    | /intake            | session      | List intake records  |
| POST   | /intake            | admin, staff | Create intake record |
| GET    | /intake/:id        | session      | Get single record    |
| PATCH  | /intake/:id        | admin, staff | Update record        |
| PATCH  | /intake/:id/status | admin, staff | Update status        |

---

## Inspections

| Method | Path                     | Auth         | Description          |
| ------ | ------------------------ | ------------ | -------------------- |
| GET    | /inspections             | session      | List inspections     |
| POST   | /inspections             | admin, staff | Create inspection    |
| PATCH  | /inspections/:id/resolve | admin, staff | Resolve with outcome |

---

## Evidence

| Method | Path                     | Auth         | Description                                    |
| ------ | ------------------------ | ------------ | ---------------------------------------------- |
| POST   | /media/upload/start      | admin, staff | Start chunked upload session                   |
| POST   | /media/upload/chunk      | admin, staff | Upload one chunk                               |
| POST   | /media/upload/complete   | admin, staff | Finalize, watermark, store                     |
| GET    | /evidence                | session      | Search by keyword, tag, date                   |
| GET    | /evidence/:id            | session      | Get evidence record                            |
| DELETE | /evidence/:id            | admin, staff | Delete unlinked evidence only (409 if linked)  |
| POST   | /evidence/:id/link       | admin, staff | Link to intake/inspection/traceability/checkin |
| PATCH  | /evidence/:id/legal-hold | admin        | Set or clear legal hold                        |

---

## Supply Entries

| Method | Path                        | Auth         | Description                |
| ------ | --------------------------- | ------------ | -------------------------- |
| GET    | /supply-entries             | session      | List entries               |
| POST   | /supply-entries             | admin, staff | Create with guided parsing |
| PATCH  | /supply-entries/:id/resolve | admin, staff | Resolve parsing conflicts  |

---

## Traceability

| Method | Path                       | Auth           | Description                                                         |
| ------ | -------------------------- | -------------- | ------------------------------------------------------------------- |
| GET    | /traceability              | session        | List codes                                                          |
| POST   | /traceability              | **admin, staff** | Create code (auditor **403** — read-only for create)              |
| POST   | /traceability/:id/publish  | admin, auditor | Publish (mandatory comment, appends `publish` step to timeline)     |
| POST   | /traceability/:id/retract  | admin, auditor | Retract (mandatory comment, appends `retract` step to timeline)     |
| GET    | /traceability/:id/steps    | session        | Ordered append-only timeline (create/publish/retract/inspection/note) |
| POST   | /traceability/:id/steps    | admin, staff   | Append manual operator note                                         |
| GET    | /traceability/verify/:code | public         | Verify checksum offline                                             |

## Transfers

Lifecycle state machine:

```
queued → approved → in_transit → received
  \          |            |
   \---------+------------+--→ canceled
```

| Method | Path                      | Auth         | Description                          |
| ------ | ------------------------- | ------------ | ------------------------------------ |
| GET    | /transfers                | session      | List (newest first)                  |
| POST   | /transfers                | admin, staff | Create in `queued` (auditor 403)     |
| GET    | /transfers/:id            | session      | Get one                              |
| PATCH  | /transfers/:id/status     | admin, staff | State machine — invalid → **409**    |

## Stock movements and inventory

| Method | Path               | Auth         | Description                                                    |
| ------ | ------------------ | ------------ | -------------------------------------------------------------- |
| GET    | /stock/movements   | session      | List movements (filters: `supply_id`, `reason`)                |
| POST   | /stock/movements   | admin, staff | Record one movement (signed `quantity_delta` + `reason`)       |
| GET    | /stock/inventory   | session      | `{ total_on_hand, by_supply }` — canonical inventory snapshot  |

`reason` ∈ `receipt | return | allocation | loss | adjustment`.
`receipt`/`return` must be positive; `allocation`/`loss` must be negative;
`adjustment` is unrestricted. Dashboard `inventory_on_hand` is
`SUM(quantity_delta) FROM stock_movements` — never `COUNT(supply_entries)`.

---

## Check-In

| Method | Path             | Auth         | Description       |
| ------ | ---------------- | ------------ | ----------------- |
| GET    | /members         | session      | List members      |
| POST   | /members         | admin, staff | Create member     |
| POST   | /checkin         | admin, staff | Check in a member |
| GET    | /checkin/history | session      | Check-in history  |

---

## Dashboard

| Method | Path                         | Auth           | Description                |
| ------ | ---------------------------- | -------------- | -------------------------- |
| GET    | /reports/summary             | session        | Metrics with filters (see below) |
| GET    | /reports/export              | admin, auditor | CSV export (staff → 403); same filter semantics as summary |

**Dashboard filter set** (same keys on summary + export):

- `from`, `to` — ISO date range on `intake_records.created_at`
- `status` — exact intake status
- `intake_type` — `animal | supply | donation`
- `region` — exact match on `intake_records.region`
- `tags` — substring match on CSV `intake_records.tags`
- `q` — full-text substring across `details`, `region`, `tags`

`inventory_on_hand` is **always** `SUM(quantity_delta) FROM stock_movements`.
| GET    | /reports/adoption-conversion | session        | Adoption conversion detail |

---

## Admin

| Method | Path                              | Auth  | Description                                                                              |
| ------ | --------------------------------- | ----- | ---------------------------------------------------------------------------------------- |
| GET    | /admin/config                     | admin | Get active config                                                                        |
| PATCH  | /admin/config                     | admin | Update config (saves new version, **caps last 10** on write)                             |
| GET    | /admin/config/versions            | admin | List last 10 versions                                                                    |
| POST   | /admin/config/rollback/:id        | admin | Restore a config version. **Cap re-applied after insert** so count stays ≤ 10.           |
| POST   | /admin/diagnostics/export         | admin | Build real diagnostic ZIP (see below). Returns `{download_id, download_url, size_bytes}` |
| GET    | /admin/diagnostics/download/:id   | admin | Download ZIP. Files older than 1 hour are removed by `diagnostics_cleanup`.              |
| GET    | /admin/jobs                       | admin | Job run history from `job_metrics`                                                       |
| GET    | /admin/logs                       | admin | Last 200 `structured_logs` rows (JSON)                                                   |
| POST   | /admin/account-purge              | admin | Trigger account purge now. Body: `{"grace_period_days": N}` (default 7). Runs `jobs::run_account_purge` transactionally. |
| POST   | /admin/security/rotate-key        | admin | Rotate AES-256-GCM key. Body: `{"new_key_hex": "..."}`. Re-encrypts every row transactionally. |

**Diagnostic ZIP layout** (PKZIP stored method, no external crate):

| File                  | Content                                                                                    |
| --------------------- | ------------------------------------------------------------------------------------------ |
| `logs.txt`            | Last 7 days of `structured_logs` (max 5000 rows), sanitized                                |
| `metrics.json`        | `job_metrics` rows (last 1000)                                                              |
| `config_history.json` | All `config_versions` with **full snapshot payloads**                                       |
| `audit_summary.csv`   | Aggregated audit counts by action; sensitive fields omitted                                 |

---

## Audit Log

| Method | Path               | Auth           | Description                          |
| ------ | ------------------ | -------------- | ------------------------------------ |
| GET    | /audit-logs        | admin, auditor | Query audit log                      |
| GET    | /audit-logs/export | admin, auditor | CSV export (sensitive fields masked) |

---

## Key Error Codes

| Code             | HTTP | When                                                       |
| ---------------- | ---- | ---------------------------------------------------------- |
| VALIDATION_ERROR | 400  | Invalid input                                              |
| UNAUTHORIZED     | 401  | No session or session expired                              |
| FORBIDDEN        | 403  | Wrong role                                                 |
| NOT_FOUND        | 404  | Resource does not exist                                    |
| CONFLICT         | 409  | Anti-passback, linked evidence delete, duplicate           |
| ANTI_PASSBACK    | 409  | Re-entry within 2 minutes (includes `retry_after_seconds`) |
| INTERNAL_ERROR   | 500  | Unexpected server error                                    |

## Idempotency Header

All mutating routes (`POST`/`PATCH`/`PUT`/`DELETE`) on the protected router
accept an optional `Idempotency-Key` header. Deduplication scope:

`method + normalized_route + actor_id + idempotency_key`

Window: **10 minutes**. On replay, the response body and status code from
the original request are returned verbatim and an `Idempotent-Replay: true`
header is set. Records are cleaned up opportunistically on each write.

Cross-actor isolation: two different users sending the same key + route +
body produce two distinct side effects — keys are namespaced by `actor_id`.

## Role Matrix

| Role               | Can do                                                                                                                                                |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `administrator`    | All endpoints, including user management, key rotation, diagnostics, legal hold, account purge                                                        |
| `operations_staff` | Intake/inspections/evidence/supply/checkin/address-book create + update. Cannot publish or retract traceability. Cannot access admin routes.          |
| `auditor`          | **Read-only** across the board, plus CSV report/audit exports and `POST /traceability` / `publish` / `retract`. Cannot mutate any operational resource. |

Enforcement is in-handler (`common::require_write_role` and
`common::require_admin_or_auditor`) in addition to route-level
`require_auth` / `require_admin` middleware.

## Account Deletion Lifecycle (FK-safe anonymization)

1. `POST /account/delete` marks `users.deletion_requested_at = now()`. The
   user retains full login access during the 7-day cooling-off window.
2. `POST /account/cancel-deletion` clears the marker.
3. After 7 days the `account_deletion_purge` background job (hourly) — or
   `POST /admin/account-purge` for an immediate operator-driven run —
   transactionally anonymizes the user:
   - Deletes all `address_book` rows for the user (personal data).
   - Deletes all active sessions for the user.
   - Nulls out `checkin_ledger.override_by` and `audit_logs.actor_id`.
   - Rewrites the `users` row in place: `username = anon-<uuid>`,
     `password_hash = '$invalid$anonymized$'`, `anonymized = 1`,
     `deletion_requested_at = NULL`.

Because the users row is preserved (just anonymized), every `NOT NULL` FK
in `intake_records.created_by`, `inspections.inspector_id`,
`evidence_records.uploaded_by`, `supply_entries.created_by`,
`traceability_codes.created_by`, `traceability_events.actor_id`, and
`config_versions.saved_by` remains valid — the historical record is
preserved but can no longer be traced back to a natural person.

Login, register bootstrap, and admin user listing all filter
`WHERE anonymized = 0` so anonymized tombstones are invisible everywhere.
