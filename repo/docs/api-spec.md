# FieldTrace API Specification

## Authentication

All protected endpoints require a valid `session_id` cookie. Sessions expire after 30 minutes of inactivity.

### POST /auth/register
Bootstrap the first administrator. Only works when no non-anonymized users exist.
- **Body**: `{ "username": string, "password": string }`
- **Response**: `201 Created` with `AuthResponse`
- **Cookie**: `session_id` set with `HttpOnly; Path=/; SameSite=Strict` (+ `Secure` when `COOKIE_SECURE=true`)

### POST /auth/login
- **Body**: `{ "username": string, "password": string }`
- **Response**: `200 OK` with `AuthResponse`
- **Errors**: `401` invalid credentials, `429` account locked (10 failures in 15 min)

### POST /auth/logout
- **Auth**: session required
- **Response**: `200 OK`, clears session cookie

### GET /auth/me
- **Auth**: session required
- **Response**: `UserResponse`

### PATCH /auth/change-password
- **Auth**: session required
- **Body**: `{ "current_password": string, "new_password": string }`
- **Response**: `200 OK`. Invalidates all other sessions.

## Account Lifecycle

### POST /account/delete
Schedule account deletion (7-day cooling-off). Idempotent.

### POST /account/cancel-deletion
Cancel a pending deletion request.

## Users (Admin only)

### GET /users
List all non-anonymized users.

### POST /users
- **Body**: `{ "username": string, "password": string, "role": string }`
- **Roles**: `administrator`, `operations_staff`, `auditor`

### PATCH /users/:id
- **Body**: `{ "role": string }`

### DELETE /users/:id

## Address Book

### GET /address-book
List addresses. Sensitive fields (street, city, phone) are masked in responses.

### POST /address-book
- **Body**: `AddressRequest` (label, street, city, state, zip_plus4, phone)
- **Response**: `201 Created` with `AddressResponse` (masked fields)

### PATCH /address-book/:id
### DELETE /address-book/:id

## Intake

### GET /intake
### POST /intake
- **Body**: `{ "intake_type": string, "details": string, "region": string, "tags": string }`

### GET /intake/:id
### PATCH /intake/:id/status
- **Body**: `{ "status": string }`

## Inspections

### GET /inspections
### POST /inspections
- **Body**: `{ "intake_id": string }`

### PATCH /inspections/:id/resolve
- **Body**: `{ "status": string, "outcome_notes": string }`

## Evidence / Media Upload

### POST /media/upload/start
- **Body**: `{ "filename": string, "media_type": "photo"|"video"|"audio", "total_size": int, "duration_seconds": int }`
- **Validation**: size limits (photo 25MB, video 150MB, audio 20MB). If `duration_seconds` exceeds the policy limit (video > 60s, audio > 120s), the request is rejected early as a convenience — but this field is advisory only.
- **Duration enforcement**: The actual duration constraint is enforced at `upload_complete` by extracting duration from the assembled file bytes (MP4 `mvhd` atom, WAV `fmt`+`data` chunks). The client-declared `duration_seconds` is NOT trusted for final acceptance. See `POST /media/upload/complete` for details.

### POST /media/upload/chunk
- **Body**: `{ "upload_id": string, "chunk_index": int, "data": string }` (data is base64-encoded)
- **Validation**: chunk payload required (non-empty), magic-byte format validation on chunk 0

### POST /media/upload/complete
- **Body**: `{ "upload_id": string, "fingerprint": string, "total_size": int, "exif_capture_time": string|null, "tags": string|null, "keyword": string|null }`
- **Fingerprint verification**: Server computes SHA-256 from assembled file bytes. If the computed fingerprint does not match the client-provided `fingerprint`, returns `409 CONFLICT` with message "Fingerprint mismatch: server-computed fingerprint does not match client-provided value".
- **Duration enforcement (server-side)**: For video/audio, the server extracts the actual duration from the assembled file bytes (MP4 `mvhd` atom or WAV `fmt`+`data` chunks). If the extracted duration exceeds the policy limit (video > 60s, audio > 120s), returns `400 VALIDATION_ERROR`. If the container format is unsupported or malformed and duration cannot be extracted, returns `400 VALIDATION_ERROR` with message "Cannot verify ... duration from uploaded file". This is an intentional fail-safe: the client-declared `duration_seconds` is **not** trusted for acceptance.
- **Media processing**: Photos are JPEG-compressed at quality 80 and the watermark is burned into the image pixels (dark stripe + white text at bottom). Video/audio are stored at original quality with watermark as metadata. The file is stored under `{evidence_id}_final` with the canonical path tracked in `storage_path` column. All metadata fields reflect the actual stored file.

### GET /evidence
Query params: `keyword`, `tag`, `from`, `to`

### DELETE /evidence/:id
Only unlinked, non-held evidence. Uploader or admin.

### POST /evidence/:id/link
- **Body**: `{ "target_type": string, "target_id": string }`
- **Validation**: target existence verified before linking.

### PATCH /evidence/:id/legal-hold
- **Body**: `{ "legal_hold": bool }`
- Admin only.

## Supply Entries

### GET /supply-entries
Returns full supply data including `stock_status`, `media_references`, `review_summary`.

### POST /supply-entries
- **Body**: `SupplyRequest`
  - `name`: required
  - `sku`: optional
  - `size`, `color`: required (parsed/normalized)
  - `price_cents`, `discount_cents`: optional
  - `notes`: string
  - `stock_status`: one of `in_stock`, `low_stock`, `out_of_stock`, `unknown` (default: `unknown`)
  - `media_references`: comma-separated evidence IDs (default: empty)
  - `review_summary`: short audit summary (default: empty)
- **Response**: `201 Created` with `SupplyResponse`

### PATCH /supply-entries/:id/resolve
- **Body**: `{ "canonical_color": string|null, "canonical_size": string|null }`

## Traceability

### GET /traceability
- **Visibility**: Auditors see only published codes. Admin/staff see all.

### POST /traceability
- **Auth**: admin/staff only (auditors get 403)
- **Body**: `{ "intake_id": string|null }`

### POST /traceability/:id/publish
- **Auth**: admin or auditor
- **Body**: `{ "comment": string }`

### POST /traceability/:id/retract
- **Auth**: admin or auditor
- **Body**: `{ "comment": string }`

### GET /traceability/:id/steps
- **Visibility policy**: Auditors can only view steps for **published** traceability codes. Draft and retracted codes return `403 FORBIDDEN` for auditors. Admin/staff can view steps regardless of code status.
- **Response**: `TraceStepResponse[]` ordered by `occurred_at ASC`

### POST /traceability/:id/steps
- **Auth**: admin/staff only
- **Body**: `{ "label": string, "details": string }`

### GET /traceability/verify/:code
- **Public** (no auth). Returns `{ "code": string, "valid": bool }`.

## Transfers

### GET /transfers
### POST /transfers
### GET /transfers/:id
### PATCH /transfers/:id/status

State machine: `queued -> approved -> in_transit -> received` (or `canceled` from any).

## Stock Movements

### GET /stock/movements
### POST /stock/movements
### GET /stock/inventory

## Check-In

### GET /members
### POST /members
### POST /checkin
### GET /checkin/history

## Profile / Privacy Preferences

### GET /profile/privacy-preferences
- **Auth**: session required
- **Response**: `PrivacyPreferencesResponse`
  - `show_email`: bool (default: true)
  - `show_phone`: bool (default: false)
  - `allow_audit_log_export`: bool (default: true)
  - `allow_data_sharing`: bool (default: false)
  - `updated_at`: timestamp
- **Behavior**: Lazy-initializes default preferences on first access. User can only read own preferences.

### PATCH /profile/privacy-preferences
- **Auth**: session required
- **Body**: `PrivacyPreferencesUpdate` (all fields optional)
  - `show_email`: bool
  - `show_phone`: bool
  - `allow_audit_log_export`: bool
  - `allow_data_sharing`: bool
- **Response**: Updated `PrivacyPreferencesResponse`
- **Isolation**: Each user's preferences are independent. Updating one user's preferences does not affect others.

## Dashboard / Reports

### GET /reports/summary
Query params: `from`, `to`, `status`, `intake_type`, `region`, `tags`, `q`

### GET /reports/export
CSV export with same filter params as summary.

### GET /reports/adoption-conversion
- Query params: `from` (ISO date), `to` (ISO date) — optional period filter
- Returns `{ total, adopted, conversion_rate }` scoped to `intake_type='animal'`

## Audit Logs

### GET /audit-logs
### GET /audit-logs/export

## Admin

### GET /admin/config
### PATCH /admin/config
### GET /admin/config/versions
### POST /admin/config/rollback/:id
### POST /admin/diagnostics/export
### GET /admin/diagnostics/download/:id
### GET /admin/jobs
### GET /admin/logs
### POST /admin/account-purge
### POST /admin/retention-purge
### POST /admin/security/rotate-key
- **Requires**: `ENCRYPTION_KEY_FILE` must be configured. Returns `400` if not set (durability enforcement).
- **Body**: `{ "new_key_hex": "<64 hex chars>" }`
- Decrypts all encrypted-at-rest fields, re-encrypts with new key, commits in one transaction.
- New key is persisted atomically to the key file after DB commit.

## Error Envelope

All errors follow the standard envelope:
```json
{
  "status": 409,
  "code": "CONFLICT",
  "message": "Fingerprint mismatch: ...",
  "trace_id": "uuid"
}
```

Status codes used: 400 (VALIDATION_ERROR), 401 (UNAUTHORIZED), 403 (FORBIDDEN), 404 (NOT_FOUND), 409 (CONFLICT), 429 (ACCOUNT_LOCKED), 500 (INTERNAL_ERROR).
