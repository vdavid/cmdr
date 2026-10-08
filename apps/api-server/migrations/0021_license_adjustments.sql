-- Every Paddle adjustment (refund, chargeback, credit, and their reversals) we've been told about,
-- written by POST /webhook/paddle on `adjustment.created` / `adjustment.updated`. One row per
-- adjustment, kept at its latest status.
--
-- Paddle's dashboard knows about the money; this table is how OUR side knows a purchase was refunded
-- and why its license was revoked (or, for a partial refund or a chargeback warning, why it wasn't).
-- `GET /admin/licenses` shows each row beside the license it belongs to.
--
-- Rows never expire, for the same reason `license_issuance` rows don't: this is the audit trail.
CREATE TABLE license_adjustments (
    adjustment_id TEXT PRIMARY KEY,      -- Paddle `adj_...`; redeliveries and updates land on the same row
    transaction_id TEXT NOT NULL,        -- Paddle `txn_...` it adjusts, the `license_issuance` key
    action TEXT NOT NULL,                -- refund | chargeback | chargeback_warning | credit | ..._reverse
    scope TEXT NOT NULL,                 -- 'full' | 'partial', resolved from `type` or the line items
    status TEXT NOT NULL,                -- pending_approval | approved | rejected | reversed
    reason TEXT,                         -- Paddle's free-text reason
    total TEXT,                          -- adjusted amount in the currency's lowest unit, as Paddle sends it
    currency_code TEXT,
    event_id TEXT,                       -- Paddle `evt_...` of the delivery that last wrote the row
    paddle_updated_at TEXT NOT NULL,     -- the adjustment's `updated_at`; an older delivery never overwrites a newer one
    received_at TEXT NOT NULL            -- ISO 8601, when we first heard of it
);

CREATE INDEX idx_license_adjustments_transaction ON license_adjustments (transaction_id);
