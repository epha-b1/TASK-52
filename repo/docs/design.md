# FieldTrace Design Document

## Architecture Overview

FieldTrace is an offline-first shelter and warehouse management system built with:

- **Backend**: Axum (Rust) REST API
- **Frontend**: Leptos (Rust/WASM, CSR)
- **Database**: SQLite (embedded, WAL mode)
- **Encryption**: AES-256-GCM for PII at rest

## Module Structure

### Backend Modules (15 total)

| Module | Purpose |
|--------|---------|
| `auth` | Registration, login, logout, sessions, password change |
| `users` | Admin-managed user CRUD |
| `address_book` | Contact management with PII masking and encryption |
| `intake` | Item/animal intake tracking with state machine |
| `inspections` | QA inspections on intake records |
| `evidence` | Media upload (chunked), fingerprint verification, legal hold |
| `supply` | Supply entries with size/color normalization |
| `traceability` | Chain-of-custody codes, publish/retract lifecycle |
| `checkin` | Member check-in with anti-passback |
| `dashboard` | Reporting metrics and CSV export |
| `transfers` | Item transfers between facilities |
| `stock` | Inventory movements ledger |
| `audit` | Append-only audit log |
| `admin` | Configuration, diagnostics, key rotation, jobs |
| `profile` | User privacy preferences (persisted, user-scoped) |

## Database Schema (13 migrations)

| Migration | Tables |
|-----------|--------|
| 0001_init | sessions, facilities |
| 0002_auth | users, auth_failures |
| 0003_address_book | address_book |
| 0004_intake_inspections | intake_records, inspections |
| 0005_evidence | evidence_records, upload_sessions, evidence_links, idempotency_keys |
| 0006_supply_traceability | supply_entries, traceability_codes, traceability_events, traceability_steps |
| 0007_checkin_dashboard | members, checkin_ledger |
| 0008_admin_audit | config_versions, audit_logs |
| 0009_account_deletion | account deletion + anonymization fields |
| 0010_anonymization | legal hold flags, structured_logs |
| 0011_evidence_retention | compression metadata columns |
| 0012_transfers_stock | transfers, stock_movements |
| 0013_duration_and_privacy | upload_sessions.duration_seconds, privacy_preferences table, supply new fields |

## Security Controls

### Evidence Fingerprint Verification

The evidence upload pipeline enforces end-to-end integrity:

1. Client uploads chunks via `POST /media/upload/chunk` with base64-encoded data
2. Server persists each chunk to `storage/uploads/<upload_id>/chunk_<index>`
3. At `POST /media/upload/complete`, server:
   a. Verifies all chunk files exist on disk
   b. Assembles chunks into `<upload_id>_final`
   c. **Computes SHA-256 hash** of the assembled file bytes
   d. Compares computed hash against client-provided `fingerprint` (case-insensitive)
   e. On mismatch: returns `409 CONFLICT` with code `CONFLICT` and message "Fingerprint mismatch"
   f. On match: proceeds with evidence record creation

This prevents silent data corruption or tampering during upload.

### Duration Policy Enforcement

Media duration limits (video <= 60s, audio <= 120s) are derived **server-side from the
uploaded file bytes**. The client-declared `duration_seconds` is used only for early
rejection at upload-start; it is never trusted for final acceptance.

At `upload_complete`, the server:

1. Reads the assembled file bytes
2. Attempts to extract duration via pure byte-level container parsing:
   - **MP4/MOV** (ISO BMFF): scans top-level atoms for `moov`, then parses `mvhd` for
     `timescale` (u32) and `duration` (u32/u64). Duration = duration_field / timescale.
   - **WAV** (RIFF/WAVE): parses `fmt ` chunk for sample_rate and block_align, then
     `data` chunk size. Duration = data_size / (sample_rate × block_align).
3. Enforces the policy:
   - Extracted duration > limit → `400 VALIDATION_ERROR`
   - Duration unextractable (unsupported format, malformed container) → `400 VALIDATION_ERROR`
     ("Cannot verify ... duration from uploaded file")

This **fail-safe** approach means: formats without reliable container-level duration metadata
(MP3, FLAC, OGG, WebM/MKV) are rejected for video/audio. No external tools (ffprobe) are
needed — the parsers operate on raw bytes with zero dependencies.

### Traceability Visibility Policy

Traceability data follows a role-based visibility model:

- **List endpoint** (`GET /traceability`): Auditors see only `published` codes
- **Steps endpoint** (`GET /traceability/:id/steps`): Same visibility policy applied:
  - Auditors: can only view steps for codes with `status = 'published'`
  - Admin/staff: can view steps for codes in any status (draft, published, retracted)
  - Non-existent codes: `404 NOT_FOUND` for all roles

### Cookie Security

Session cookies include:
- `HttpOnly` — prevents JavaScript access
- `SameSite=Strict` — prevents CSRF
- `Path=/` — scoped to the application
- `Max-Age=1800` — 30-minute session window
- `Secure` — **only when `COOKIE_SECURE=true`** (production HTTPS mode)

The `Secure` flag is config-driven via the `COOKIE_SECURE` environment variable to maintain
local HTTP development usability while hardening production deployments.

### Privacy Preferences

User-scoped privacy preferences are stored in the `privacy_preferences` table:

- **Schema**: `user_id` (PK, FK to users), `show_email`, `show_phone`, `allow_audit_log_export`, `allow_data_sharing`, `updated_at`
- **Lazy initialization**: Default row created on first GET
- **User isolation**: Each user can only read/write their own preferences
- **Partial updates**: PATCH accepts any subset of fields
- **Audit trail**: Changes logged via `profile.privacy_updated` audit event

### Supply Data Model

Supply entries carry first-class fields for operational completeness:

| Field | Type | Description |
|-------|------|-------------|
| `stock_status` | enum | `in_stock`, `low_stock`, `out_of_stock`, `unknown` |
| `media_references` | text | Comma-separated evidence IDs |
| `review_summary` | text | Short audit review note |

Validated at creation: `stock_status` must be a recognized value (400 on invalid).

## Intake Status Transitions and Adoption Semantics

Intake records follow a type-aware state machine:

```
received → in_care → adopted (ANIMAL ONLY)
received → in_care → transferred
received → in_care → disposed
received → in_stock → transferred
received → in_stock → disposed
```

The `adopted` status is restricted to `intake_type = 'animal'`. Supply and
donation records cannot be adopted — the endpoint returns `400 VALIDATION_ERROR`
if this is attempted.

The adoption conversion KPI (`/reports/adoption-conversion` and the dashboard
summary) counts only `intake_type = 'animal' AND status = 'adopted'` in the
numerator, with all animal records as the denominator. This ensures the metric
is not skewed by non-animal records.

## Evidence Storage (No In-Process Transcoding)

The backend stores original uploaded files **unchanged** on disk. Real media
transcoding (JPEG re-encode, H.264, AAC) is NOT performed in-process — that
requires a full media codec library (ffmpeg/libavcodec) outside the current
dependency scope.

Compression metadata fields reflect the **actual stored file**:
- `compressed_bytes` = real file size on disk (equals original)
- `compression_ratio` = 1.0 (no compression performed)
- `compression_applied` = false

These fields are reserved for future integration with an external offline
transcoding pipeline.

## Frontend Architecture

Leptos CSR SPA with:
- Draft autosave to localStorage
- Session-expiry route preservation
- Role-aware UI rendering
- 14 page components including profile with privacy preferences editing

## Middleware Stack

Request processing order:
1. `trace_id` — UUID generation, X-Trace-Id header
2. `session` — Cookie extraction, DB validation, 30-min window
3. `auth_guard` — SessionUser enforcement (401/403)
4. `idempotency` — Replay for duplicate Idempotency-Key headers
