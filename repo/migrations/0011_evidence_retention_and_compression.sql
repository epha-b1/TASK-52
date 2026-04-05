-- Remediation: media compression metadata + retention-scan index.

-- Compression fields populated by evidence::upload_complete.
-- compression_applied is 1 when the policy produced a smaller payload,
-- 0 when the policy decided to keep the original (e.g. already-compressed
-- formats or media types outside the policy). compressed_bytes is the
-- size after local policy compression; compression_ratio is
-- compressed_bytes / original size_bytes.
ALTER TABLE evidence_records ADD COLUMN compressed_bytes INTEGER;
ALTER TABLE evidence_records ADD COLUMN compression_ratio REAL;
ALTER TABLE evidence_records ADD COLUMN compression_applied INTEGER NOT NULL DEFAULT 0;

-- Retention scans filter by (created_at, linked, legal_hold) on every pass.
-- A composite index cuts the retention_purge cost on large tables to a
-- range scan rather than a full table sweep.
CREATE INDEX IF NOT EXISTS idx_evidence_retention
    ON evidence_records(created_at, linked, legal_hold);
