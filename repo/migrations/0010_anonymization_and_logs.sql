-- Remediation: purge-safe account deletion via in-place anonymization.
--
-- Strategy: instead of hard-deleting the users row (which would orphan
-- every FK in intake_records, inspections, evidence_records, supply_entries,
-- traceability_codes, traceability_events, config_versions, etc.), we
-- preserve the row as an "anonymized tombstone" with:
--   - personal data stripped (address_book entries deleted, username rotated)
--   - a non-verifiable password hash so login is impossible
--   - anonymized = 1 flag so login/register/list exclude the row
--
-- Evidence and traceability history remain auditable. Personal references
-- now point to an anonymized tombstone instead of a natural person.

ALTER TABLE users ADD COLUMN anonymized INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_users_anonymized ON users(anonymized);
