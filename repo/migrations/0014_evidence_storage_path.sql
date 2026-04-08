-- Track the canonical storage path for each evidence file so delete/retention
-- can reliably clean up the on-disk artifact regardless of which ID was used
-- during the upload session.
ALTER TABLE evidence_records ADD COLUMN storage_path TEXT NOT NULL DEFAULT '';
