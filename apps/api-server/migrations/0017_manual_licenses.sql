-- Manual licenses live in `license_issuance` beside the Paddle fulfillments, which makes the table
-- the single ledger of every license this server has ever issued.
--
-- A manually issued license has no Paddle transaction to resolve against, so `POST /validate`
-- answers for it from these columns instead: `source = 'manual'` rows are looked up by
-- `transaction_id` (a `manual-...` id, never a Paddle `txn_...`), and the row alone decides active
-- versus expired versus invalid. The Paddle path is untouched and still resolves against Paddle.
--
-- Existing rows are all Paddle fulfillments, and the column default backfills them as such.
ALTER TABLE license_issuance ADD COLUMN source TEXT NOT NULL DEFAULT 'paddle'; -- 'paddle' | 'manual'

-- Who the license is for. On a Paddle row this lives in the transaction's `custom_data`; a manual
-- row has nowhere else to keep it, and `/validate` returns it so the app can show "Licensed to".
ALTER TABLE license_issuance ADD COLUMN organization_name TEXT;

-- ISO 8601, NULL = perpetual. Only read for manual rows: a Paddle license expires when its
-- subscription does, which only Paddle knows.
ALTER TABLE license_issuance ADD COLUMN expires_at TEXT;

-- ISO 8601, set by POST /admin/revoke. A revoked manual license validates as invalid, so the app
-- drops to Personal at its next revalidation (within 7 days). Paddle licenses are revoked by
-- canceling the subscription in Paddle, never here.
ALTER TABLE license_issuance ADD COLUMN revoked_at TEXT;

-- Why the license was issued, for example "Test Customer, Acme, evaluation". Required when
-- minting a manual license: a free license nobody can explain later is worse than no record.
ALTER TABLE license_issuance ADD COLUMN note TEXT;
