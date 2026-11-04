-- ISO 8601, set when the "a new Cmdr sale" email to David went out for this purchase (`src/licensing/sale-notices.ts`).
-- Claimed with a conditional UPDATE before the send, so Paddle's redeliveries and a rerun backfill mail each sale once.
-- A failed send clears it again, so the backfill route can retry it. NULL on every row that predates the notice.
ALTER TABLE license_issuance ADD COLUMN sale_notified_at TEXT;
