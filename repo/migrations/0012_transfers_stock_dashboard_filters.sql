-- Remediation batch: transfers + stock_movements + intake filter columns.

-- ── Transfers ─────────────────────────────────────────────────────────
-- First-class operational queue for moving intake items (animals, supplies,
-- donations) between facilities or between internal locations. Replaces
-- the workspace "filter intake by status" shortcut.
--
-- Lifecycle state machine:
--   queued → approved → in_transit → received
--   queued | approved | in_transit → canceled
-- Any other transition returns 409.
CREATE TABLE IF NOT EXISTS transfers (
    id TEXT PRIMARY KEY NOT NULL,
    intake_id TEXT REFERENCES intake_records(id),
    origin_facility_id TEXT NOT NULL DEFAULT 'default',
    destination TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK(status IN ('queued','approved','in_transit','received','canceled')),
    notes TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_transfers_status ON transfers(status, created_at);
CREATE INDEX IF NOT EXISTS idx_transfers_intake ON transfers(intake_id);

-- ── Stock movements ledger ────────────────────────────────────────────
-- Append-only ledger replacing the "COUNT(supply_entries)" inventory
-- shortcut. Every change in inventory (receipt, allocation, adjustment)
-- produces one row with a signed quantity_delta. Current inventory_on_hand
-- is SUM(quantity_delta). Per-supply quantity = SUM(quantity_delta) WHERE
-- supply_id = ?.
CREATE TABLE IF NOT EXISTS stock_movements (
    id TEXT PRIMARY KEY NOT NULL,
    supply_id TEXT REFERENCES supply_entries(id),
    quantity_delta INTEGER NOT NULL,
    reason TEXT NOT NULL
        CHECK(reason IN ('receipt','allocation','adjustment','return','loss')),
    notes TEXT NOT NULL DEFAULT '',
    actor_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_stock_supply ON stock_movements(supply_id, created_at);

-- ── Intake filter columns ─────────────────────────────────────────────
-- Dashboard needs region/tags/full-text filtering per prompt. The
-- existing intake_records.details JSON blob is not queryable, so we add
-- concrete columns that can be indexed and searched.
ALTER TABLE intake_records ADD COLUMN region TEXT NOT NULL DEFAULT '';
ALTER TABLE intake_records ADD COLUMN tags TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_intake_region ON intake_records(region);
CREATE INDEX IF NOT EXISTS idx_intake_tags ON intake_records(tags);
