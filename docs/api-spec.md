# FieldTrace API Spec

Base URL: `http://localhost:8080`
Auth model: HttpOnly session cookie after login.

## Public Routes (no auth)

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/health` | Health check (`{"status":"ok"}`) |
| POST | `/auth/register` | First admin bootstrap only; 409 after initialization |
| POST | `/auth/login` | Creates session cookie; returns `AuthResponse` |
| GET | `/traceability/verify/:code` | Offline Luhn checksum verification (public) |

## Protected Routes (session required)

### Auth & Identity

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/auth/logout` | Invalidates active session |
| GET | `/auth/me` | Returns current principal |
| PATCH | `/auth/change-password` | Enforces min length (12 chars) and policy checks |
| POST | `/account/delete` | Schedule account deletion (7-day cooling-off) |
| POST | `/account/cancel-deletion` | Cancel pending deletion |

### Address Book

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/address-book` | List user's addresses (user-scoped) |
| POST | `/address-book` | Create address (auditor: 403) |
| PATCH | `/address-book/:id` | Update address (owner + non-auditor) |
| DELETE | `/address-book/:id` | Delete address (owner + non-auditor) |

### Intake

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/intake` | List intake records |
| POST | `/intake` | Create intake record (non-auditor) |
| GET | `/intake/:id` | Get single intake record |
| PATCH | `/intake/:id/status` | Update status (enforces valid transitions; 409 on invalid) |

### Inspections

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/inspections` | List inspections |
| POST | `/inspections` | Create inspection (non-auditor) |
| PATCH | `/inspections/:id/resolve` | Resolve once (pending → passed/failed) |

### Evidence & Media Upload

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/media/upload/start` | Begin chunked upload session |
| POST | `/media/upload/chunk` | Upload single chunk (base64 data, format validated on chunk 0) |
| POST | `/media/upload/complete` | Finalize upload, apply compression, create evidence record |
| GET | `/evidence` | List evidence (filters: keyword, tag, from, to) |
| DELETE | `/evidence/:id` | Delete unlinked evidence (uploader or admin) |
| POST | `/evidence/:id/link` | Link to intake/inspection/traceability/checkin (validates target exists) |
| PATCH | `/evidence/:id/legal-hold` | Toggle legal hold (admin only) |

### Supply

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/supply-entries` | List supply entries |
| POST | `/supply-entries` | Create with auto color/size normalization (non-auditor) |
| PATCH | `/supply-entries/:id/resolve` | Resolve needs_review → ok |

### Traceability

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/traceability` | List codes (auditor: published only) |
| POST | `/traceability` | Generate code with Luhn checksum (non-auditor) |
| POST | `/traceability/:id/publish` | Publish (admin/auditor) |
| POST | `/traceability/:id/retract` | Retract (admin/auditor) |
| GET | `/traceability/:id/steps` | List timeline steps |
| POST | `/traceability/:id/steps` | Append manual step (non-auditor) |

### Transfers

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/transfers` | List transfers (newest first) |
| POST | `/transfers` | Create transfer in queued state |
| GET | `/transfers/:id` | Get single transfer |
| PATCH | `/transfers/:id/status` | Update status (enforces state machine; 409 on invalid) |

### Stock Movements

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/stock/movements` | List movements (filter: supply_id, reason) |
| POST | `/stock/movements` | Record movement with signed delta (non-auditor) |
| GET | `/stock/inventory` | Snapshot: `{total_on_hand, by_supply}` |

### Check-In

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/members` | List members |
| POST | `/members` | Create member |
| POST | `/checkin` | Check in member (2-min anti-passback; admin override) |
| GET | `/checkin/history` | Recent check-in history |

### Dashboard & Reports

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/reports/summary` | Metrics with filter set (from, to, status, intake_type, region, tags, q) |
| GET | `/reports/export` | CSV export with same filters (admin/auditor only) |
| GET | `/reports/adoption-conversion` | Adoption conversion metrics |

### Audit

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/audit-logs` | List last 200 audit log entries (admin/auditor only) |
| GET | `/audit-logs/export` | CSV export (admin/auditor; [REDACTED] masking) |

## Admin-Only Routes (administrator role required)

| Method | Path | Notes |
| --- | --- | --- |
| GET | `/users` | List all non-anonymized users |
| POST | `/users` | Create user |
| PATCH | `/users/:id` | Update user |
| DELETE | `/users/:id` | Delete user (soft anonymization) |
| GET | `/admin/config` | Get latest config version |
| PATCH | `/admin/config` | Save new config (cap: 10 versions) |
| GET | `/admin/config/versions` | List recent config versions |
| POST | `/admin/config/rollback/:id` | Restore old config version |
| POST | `/admin/diagnostics/export` | Build diagnostics ZIP |
| GET | `/admin/diagnostics/download/:id` | Download ZIP (1-hour expiry) |
| GET | `/admin/jobs` | Job metrics |
| GET | `/admin/logs` | Last 200 structured log rows |
| POST | `/admin/account-purge` | Immediate anonymization (grace_period_days param) |
| POST | `/admin/retention-purge` | Immediate evidence deletion sweep (max_age_days param) |
| POST | `/admin/security/rotate-key` | Decrypt all fields, re-encrypt with new key |

## Error Codes

| Code | HTTP Status | Description |
| --- | --- | --- |
| `VALIDATION_ERROR` | 400 | Invalid input |
| `UNAUTHORIZED` | 401 | Missing/expired session |
| `FORBIDDEN` | 403 | Insufficient role |
| `NOT_FOUND` | 404 | Resource not found |
| `CONFLICT` | 409 | Invalid state transition or duplicate |
| `ACCOUNT_LOCKED` | 429 | Too many failed login attempts |
| `ANTI_PASSBACK` | 409 | Re-entry blocked (includes `retry_after_seconds`) |
| `INTERNAL_ERROR` | 500 | Server error (details logged, not exposed) |

## Environment Variables

| Variable | Default | Description |
| --- | --- | --- |
| `PORT` | 8080 | Server port |
| `DATABASE_URL` | sqlite://app.db | SQLite database path |
| `STATIC_DIR` | static | Frontend static files directory |
| `ENCRYPTION_KEY` | (auto-generated) | 64-char hex (32 bytes AES-256); auto-generated for dev if missing |
| `ENCRYPTION_KEY_FILE` | (none) | Path to key file; authoritative when set |
| `STORAGE_DIR` | /app/storage | Uploads, diagnostics, exports directory |
| `FACILITY_CODE` | FAC01 | Facility code for watermarks and traceability codes |
