# Required Document Description: Business Logic Questions Log

This file records business-level ambiguities from the prompt and implementation decisions.
Each entry follows exactly: Question + My Understanding/Hypothesis + Solution.

## 1) Intake Record — What Is It Exactly?
Question: The prompt mentions "intake record" as something evidence can be linked to. What does an intake record represent in this system?
My Understanding/Hypothesis: An intake record represents a single animal or supply batch entering the facility. For animals: species, intake date, condition, source. For supplies: item type, quantity, source. Each intake gets a unique ID that can be linked to evidence, inspections, and traceability codes.
Solution: `intake_records` table: `id`, `type` (animal/supply), `facility_id`, `intake_date`, `status`, `details` (JSON), `created_by`. Evidence and traceability records FK to `intake_id`.

## 2) Traceability Code — What Does It Encode?
Question: Traceability codes are generated locally with a checksum. What information is encoded in the code, and what format is it?
My Understanding/Hypothesis: The code encodes: facility code + intake ID + sequence number + checksum digit. Format: `{FACILITY}-{YYYYMMDD}-{SEQ4}-{CHECK}` e.g. `FAC01-20260401-0042-7`. The checksum is a Luhn-style digit computed from the preceding segments. Verification is purely local — no external lookup.
Solution: `generateTraceabilityCode(facilityCode, intakeId, seq)` produces the code. `verifyTraceabilityCode(code)` recomputes the checksum and compares. Codes are stored in `traceability_codes` table with `status` (active/retracted) and `version`.

## 3) Publish vs Retract — What Does "Public-Facing" Mean?
Question: Retraction "immediately hides public-facing views." Since this is an offline system with no internet, what is the public-facing view?
My Understanding/Hypothesis: "Public-facing" means the read-only view accessible to Auditors and non-staff users within the local network. Published traceability records are visible to Auditors. Retracted records are hidden from Auditor views but remain in the database with full audit trail.
Solution: `traceability_codes.status` enum: `draft | published | retracted`. Auditor queries filter `status = published`. Admin/Operations Staff can see all statuses. Every publish/retract stores a `traceability_events` record with the mandatory comment and actor.

## 4) Anti-Passback — What Scope Does It Apply To?
Question: Anti-passback prevents re-entry within 2 minutes. Is this per facility, per door/gate, or system-wide?
My Understanding/Hypothesis: Per facility. A member ID cannot check in to the same facility twice within 2 minutes. Different facilities are independent. The 2-minute window is measured from the last successful check-in timestamp for that member+facility combination.
Solution: `checkin_ledger` table: `member_id`, `facility_id`, `checked_in_at`. Before allowing check-in, query: `SELECT MAX(checked_in_at) WHERE member_id=? AND facility_id=? AND checked_in_at > now() - 2 minutes`. If found, return 409 with `retryAfterSeconds`.

## 5) Account Deletion Cooling-Off — What Happens During 7 Days?
Question: Account deletion has a 7-day cooling-off period. Can the user still log in and use the system during those 7 days? Can they cancel the deletion?
My Understanding/Hypothesis: During the 7-day period the account is marked `pending_deletion`. The user can still log in and cancel the deletion. After 7 days a scheduled job permanently deletes the account and all associated personal data. Evidence linked to traceability records is anonymized rather than deleted.
Solution: `users.deletion_requested_at` timestamp. Login checks: if `deletion_requested_at` is set, show a warning banner with a "Cancel deletion" button. `POST /account/cancel-deletion` clears the field. A daily cron job hard-deletes accounts where `deletion_requested_at < now() - 7 days`.

## 6) Configuration Rollback — What Is a "Configuration"?
Question: Operators can roll back configuration to any of the last 10 saved versions. What counts as a configuration?
My Understanding/Hypothesis: Configuration includes: risk keyword library, parsing rules (color normalization map, size conversion rules), system settings (session timeout, retention period, max file sizes), and role permission assignments. Each save creates a versioned snapshot. Rolling back restores all config fields to the selected snapshot.
Solution: `config_versions` table: `id`, `version_number`, `snapshot` (JSON), `saved_by`, `created_at`. Max 10 versions kept (oldest deleted on overflow). `POST /admin/config/rollback/:versionId` restores the snapshot and creates a new version entry recording the rollback.

## 7) Watermark — Is It Burned Into the File or Overlaid in UI?
Question: The UI stamps a visible watermark on captured media. Is this burned into the image/video file, or just displayed as an overlay in the UI?
My Understanding/Hypothesis: For photos, the watermark is burned into the image file server-side using image processing (text overlay with facility code and timestamp). For videos and audio, the watermark metadata is stored as a separate record rather than re-encoding the file. The UI always displays the watermark data alongside the media.
Solution: On photo upload, the server applies a text watermark using an image library. `evidence_records.watermark_text` stores the stamped text. For video/audio, `watermark_text` is stored but not burned in. The frontend renders the watermark as an overlay for video/audio.

## 8) Resumable Uploads — How Are Chunks Tracked?
Question: Media ingestion supports resumable uploads in 2 MB chunks. How does the server track which chunks have been received?
My Understanding/Hypothesis: The client sends a unique `upload_id` with each chunk request, along with `chunk_index` and `total_chunks`. The server stores received chunks and returns a list of received chunk indices. The client can resume by re-sending missing chunks. Once all chunks are received, the server assembles the file.
Solution: `upload_sessions` table: `upload_id`, `filename`, `total_chunks`, `received_chunks` (JSON array of indices), `status` (in_progress/complete/failed). `POST /media/upload/chunk` accepts `upload_id`, `chunk_index`, binary data. `POST /media/upload/complete` triggers assembly and fingerprint validation.

## 9) Structured Parsing Conflicts — What Triggers "Needs Review"?
Question: Conflicts in structured parsing trigger a "needs review" state. What exactly constitutes a conflict?
My Understanding/Hypothesis: A conflict occurs when: (1) a field value cannot be mapped to a canonical enum (e.g., color "teal" has no mapping), (2) a size value has ambiguous units (e.g., "12" with no unit specified), or (3) a required field is missing and no local default exists. Items in "needs review" are visible to Operations Staff for manual resolution.
Solution: `supply_entries.parse_status` enum: `ok | needs_review`. `parse_conflicts` JSON field lists each conflicting field with the raw value and reason. Operations Staff can resolve conflicts via `PATCH /supply-entries/:id/resolve`.

## 10) Diagnostic Package — What Exactly Is Included?
Question: The one-click diagnostic package exports logs, metrics snapshot, and config history as a ZIP. What time range of logs is included?
My Understanding/Hypothesis: The last 7 days of structured logs, the current metrics snapshot (job run counts, error rates, queue depths), and all 10 config version snapshots. The ZIP is generated on demand and available for download for 1 hour before being deleted.
Solution: `POST /admin/diagnostics/export` triggers ZIP generation: collects log entries from `structured_logs` where `created_at > now() - 7 days`, current metrics from `job_metrics`, all rows from `config_versions`. Returns a download URL. A cleanup job deletes the file after 1 hour.

## 11) Adoption Conversion Metric — What Does It Measure?
Question: The dashboard shows "adoption conversion" as a metric. What is the conversion from and to?
My Understanding/Hypothesis: Adoption conversion = number of animals that moved from intake status to "adopted" status, divided by total intakes in the period. Expressed as a percentage. Only applies to animal intake records, not supply batches.
Solution: `GET /reports/adoption-conversion?from=&to=&facilityId=` queries `intake_records` where `type=animal`, counts total and those with `status=adopted` in the period. Returns `{ total, adopted, conversionRate }`.

## 12) Evidence Immutability — Can Evidence Be Deleted?
Question: Evidence links are immutable once referenced by a traceability record. But what about evidence that has never been linked? Can it be deleted?
My Understanding/Hypothesis: Unlinked evidence can be deleted by the Operations Staff who uploaded it, or by an Administrator. Once linked to any traceability record, the evidence record becomes immutable — it cannot be deleted or modified, only the link can be retracted (which hides it from public view but preserves the record). After the 365-day retention period, unlinked evidence is auto-deleted unless a legal hold is active.
Solution: `evidence_records.linked` boolean. Delete endpoint checks `linked = false`. Retention job skips records where `legal_hold = true` or `linked = true`. Retraction sets `traceability_evidence_links.retracted = true` without touching the evidence record.

## 13) Roles & Permissions — What Can Each Role Actually Do?
Question: The prompt names three roles (Administrator, Operations Staff, Auditor) and mentions a few specific restrictions (publish/retract is Admin/Auditor only, facial recognition is a placeholder). What is the full permission matrix — which roles can create, edit, delete, or view each major resource?
My Understanding/Hypothesis: Administrator has full access to everything. Operations Staff can create/edit intake records, evidence, supply entries, and check-ins but cannot publish/retract traceability codes or access admin config. Auditor has read-only access to published traceability records, intake records, and dashboard reports — no write access anywhere.
Solution: A `role_permissions` map enforced server-side on every endpoint. Middleware checks `session.role` against the required permission for the route before processing the request.

## 14) Multi-Facility — How Many Facilities Does One Instance Serve?
Question: The prompt references "facility code" in watermarks and check-in ledger, and the dashboard has a "region" filter. Does a single SQLite instance serve one facility or multiple? Can a user belong to multiple facilities?
My Understanding/Hypothesis: A single instance serves one facility (single-node, offline). The "region" filter on the dashboard is for grouping/reporting purposes within that facility, not for cross-facility queries. A user belongs to one facility.
Solution: `facilities` table with a single row in practice. `facility_id` is a FK on most tables for forward-compatibility but queries don't need to join across facilities.

## 15) Member ID — Who Are "Members"?
Question: The check-in ledger scans a "member ID." Who are members — are they animals, people (adopters/visitors), or staff? Are members stored in the system or just referenced by ID?
My Understanding/Hypothesis: Members are people (adopters, volunteers, or visitors) who check in to the facility. They are distinct from staff users who log in with credentials. Members have a minimal record: ID, name, and check-in history. They do not have system login access.
Solution: `members` table: `id`, `member_id` (barcode value), `name`, `created_at`. Check-in ledger FKs to `members.id`. Member records are created/managed by Operations Staff or Administrators.

## 16) Inspections — What Is Being Inspected?
Question: The home workspace shows "pending inspections" and evidence can be linked to an "inspection result." What is the subject of an inspection — an animal, a supply item, a facility area? Who creates and who resolves an inspection?
My Understanding/Hypothesis: Inspections apply to intake records (animal health checks or supply quality checks). Operations Staff create inspections and record outcomes. An inspection has a status (pending/passed/failed) and can have evidence attached. Administrators can view all inspections; Auditors can view completed ones.
Solution: `inspections` table: `id`, `intake_id`, `inspector_id`, `status`, `outcome_notes`, `created_at`, `resolved_at`. Evidence FKs to `inspection_id` optionally.

## 17) Offline Sync — Is There Any Sync at All?
Question: The system is described as fully offline. Is there any scenario where data is exported or synced to another instance (e.g., a regional hub), or is the SQLite database truly the final destination with no outbound data flow beyond CSV export and the diagnostic ZIP?
My Understanding/Hypothesis: No sync. The SQLite database is the final destination. The only outbound data flows are: CSV export from the dashboard, the diagnostic ZIP, and the traceability code itself (which can be printed/shared as a string). There is no replication, no REST push to external systems.
Solution: No sync infrastructure needed. Document this explicitly so no one adds an outbound HTTP client expecting it to work.

## 18) Session Expiry — What Happens to In-Progress Work?
Question: Sessions expire after 30 minutes of inactivity. If a user is mid-way through filling out a form (e.g., a supply entry or intake record) and the session expires, what happens to their unsaved data?
My Understanding/Hypothesis: The frontend saves form state to `localStorage` as a draft. On session expiry the user is redirected to the login page. After re-authenticating, the draft is restored from `localStorage` so no data is lost. The server never receives partial writes — only complete, validated submissions.
Solution: Leptos form components write to `localStorage` on every field change. A session-expiry interceptor on the API client detects 401 responses, stores the current route, redirects to login, and restores the route post-auth.

## 19) Encryption Key Management — How Is the Local Key Stored and Rotated?
Question: Sensitive fields are encrypted at rest with a "locally managed key." Where is this key stored, how is it initialized on first run, and can it be rotated without data loss?
My Understanding/Hypothesis: The key is a 256-bit AES key stored in a local key file (e.g., `data/keystore.bin`) outside the SQLite database, with file-system permissions restricting access to the process user. On first run the key is generated and written. Key rotation re-encrypts all sensitive fields in a single transaction and writes the new key atomically.
Solution: `keystore` module: `init_key()` generates or loads the key. `encrypt(value)` / `decrypt(value)` use AES-256-GCM. `rotate_key(new_key)` wraps re-encryption in a SQLite transaction. Admins trigger rotation via `POST /admin/security/rotate-key`.

## 20) Address Book — Is It Per-User or Shared?
Question: Users maintain a "local address book for shipping destinations." Is this address book private to each user, or is it shared across all staff at the facility?
My Understanding/Hypothesis: The address book is per-user. Each staff member maintains their own list of shipping destinations. Administrators cannot view other users' address books. Addresses are encrypted at rest as sensitive fields.
Solution: `address_book` table: `id`, `user_id`, `label`, `street`, `city`, `state`, `zip_plus4`, `phone` (masked), `created_at`. All queries filter by `user_id = session.user_id`.

## 21) CSV Export — What Data Is Exported and Who Can Export?
Question: The dashboard supports CSV export for metrics. Does the export contain raw records (individual intake rows, check-in events) or aggregated metric summaries? And which roles can trigger an export?
My Understanding/Hypothesis: The export contains aggregated metric summaries matching what is visible on the dashboard (rescue volume, adoption conversion rate, task completion rate, donations logged, inventory on hand) for the selected filter range. Raw record export is not included. Administrators and Auditors can export; Operations Staff cannot.
Solution: `GET /reports/export?from=&to=&facilityId=&tags=` returns a CSV of aggregated metrics. Role check: `role IN (admin, auditor)`. CSV columns match the dashboard metric set.

## 22) Donations Logged Offline — What Is a Donation?
Question: The dashboard metric "donations logged offline" is mentioned but the prompt doesn't describe a donation workflow. What does a donation represent — monetary, in-kind supplies, or both? Who logs it and what fields does it have?
My Understanding/Hypothesis: Donations are in-kind supply donations (physical items brought to the facility), not monetary. They are logged by Operations Staff as a special intake type with a donor reference (name/org, anonymized if requested). The metric counts the number of donation intake records in the period.
Solution: `intake_records.type` gains a `donation` variant. `intake_records.donor_ref` (nullable, encrypted) stores the donor identifier. Dashboard metric counts `type=donation` records in the selected period.

## 23) Transfer Queue — What Is a "Transfer" and What States Exist?
Question: The home workspace includes a transfer queue, but transfer is not defined. Is a transfer movement between facilities, movement to an adopter, or both? What lifecycle states does a transfer follow?
My Understanding/Hypothesis: A transfer is an operational movement of an intake record between internal facility zones or between facilities; adoption is a separate terminal intake status and not treated as a transfer. A transfer request moves through `queued | approved | in_transit | received | canceled`.
Solution: Add `transfers` table: `id`, `intake_id`, `from_facility_id`, `to_facility_id`, `reason`, `status`, `requested_by`, `approved_by`, `departed_at`, `received_at`, `created_at`. Home workspace transfer queue reads open transfers (`queued/approved/in_transit`) and exceptions include stale `in_transit` records past SLA.

## 24) Task Completion Metric — What Counts as a "Task"?
Question: Dashboard metrics include task completion rate, but "task" is undefined. Is it derived from inspections/check-ins/supply entries, or does the system have a dedicated task entity?
My Understanding/Hypothesis: Task completion rate is calculated from a dedicated `tasks` entity to avoid conflating unrelated workflows. Tasks can optionally reference source records (inspection, check-in, supply entry) but are still first-class records with clear due/completed timestamps.
Solution: Add `tasks` table: `id`, `facility_id`, `title`, `category`, `source_type`, `source_id`, `assignee_user_id`, `status` (`open|in_progress|completed|canceled`), `due_at`, `completed_at`, `created_at`. Metric formula: `completed_in_period / (completed_in_period + overdue_open_in_period)` and expose by filter range/region/tags.

## 25) Inventory on Hand — How Is It Computed?
Question: The dashboard metric "inventory on hand" is required, but data origin is ambiguous. Is inventory derived from current `supply_entries.stock_status`, or from explicit stock movement records?
My Understanding/Hypothesis: Inventory on hand should be computed from explicit stock movement records for auditability. `stock_status` on supply entries is a presentation snapshot, not the accounting source of truth.
Solution: Add `stock_movements` table: `id`, `supply_entry_id`, `movement_type` (`intake|adjustment|allocation|transfer_out|transfer_in|disposal`), `quantity_delta`, `unit`, `reason`, `performed_by`, `created_at`. Inventory on hand for a scope = `SUM(quantity_delta)` grouped by supply item; `supply_entries.stock_status` is updated from movement aggregates.

## 26) Traceability Process Steps — What Exactly Are They?
Question: Traceability records aggregate "process steps," but it is unclear whether steps are manual logs, automatic status transitions, inspection outcomes, or a mix.
My Understanding/Hypothesis: Process steps are an append-only timeline combining: (1) system-generated transitions (intake status changes, transfer status changes), (2) linked inspection outcomes, and (3) explicit manual step notes authored by staff.
Solution: Add `traceability_steps` table: `id`, `traceability_code_id`, `step_type` (`auto_transition|inspection_outcome|manual_note`), `source_type`, `source_id`, `step_label`, `details_json`, `occurred_at`, `created_by`. Traceability publish payload includes ordered steps plus linked inspection summaries for offline verification.

## 27) Account Lockout — What Are N and the Rolling Window?
Question: Security requires account lockout after repeated failures in a rolling window, but the threshold values are not specified. What are `N` and the lockout/reset durations?
My Understanding/Hypothesis: Lock account after 10 failed login attempts within 15 minutes. Lock duration is 15 minutes from the triggering attempt. Successful login resets the failure window.
Solution: Add `auth_failures` table with timestamped failures and enforce lockout with `COUNT(*) WHERE username=? AND attempted_at > now()-15m`. Return `429` or `423` (project standard) with `retry_after_seconds` when locked.

## 28) Idempotency — Which Endpoints and What Window?
Question: Workflow requires idempotency for retryable mutating operations, but prompt does not enumerate exact endpoints or dedup window.
My Understanding/Hypothesis: Apply idempotency to high-risk create/submit endpoints (`/intake`, `/media/upload/complete`, `/traceability/:id/publish`, `/traceability/:id/retract`, `/checkin`) with a 10-minute dedup window.
Solution: Store idempotency records keyed by `method + normalized_route + actor_id + idempotency_key`, persist response snapshot, and enforce middleware ordering so auth executes first. Replays within window return the original response.

## 29) User Registration — Is Self-Register Allowed or Admin-Only Provisioning?
Question: Prompt says users sign in locally and have roles, but does not fully define how accounts are created. Is there a public registration endpoint, admin-only user creation, or both?
My Understanding/Hypothesis: Support a controlled `POST /auth/register` endpoint for initial bootstrap (first Administrator creation only), then require Administrator-managed `POST /users` for ongoing account provisioning.
Solution: Add bootstrap guard (`system_initialized` flag or first-user check): - If no users exist, `POST /auth/register` creates first admin and returns session. - After bootstrap, `POST /auth/register` returns 403/409 and account creation flows through admin-only `POST /users`.
