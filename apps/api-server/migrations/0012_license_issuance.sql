-- One row per paid Paddle transaction we have fulfilled, written by POST /webhook/paddle.
-- The primary key is what makes issuance exactly-once: Paddle redelivers the same event (60
-- attempts over 3 days on live) and a captured webhook can be replayed, so two deliveries race
-- on this key and SQLite hands the row to exactly one of them.
--
-- Rows NEVER expire. "This purchase was fulfilled" has no useful end date, and an expiring
-- record would let a late redelivery mint a second set of usable licenses.
--
-- The three timestamps are the fulfillment state machine (see `src/license-issuance.ts`):
-- claimed_at = a delivery owns it, issued_at = codes minted and stored in KV, emailed_at = the
-- license email was accepted. Minting is exactly-once; emailing is safe to repeat, so a delivery
-- that finds stored codes re-sends those instead of minting more.
CREATE TABLE license_issuance (
    transaction_id TEXT PRIMARY KEY,     -- Paddle `txn_...`, the unit of fulfillment
    event_id TEXT,                       -- Paddle `evt_...` of the delivery that claimed it
    short_codes TEXT NOT NULL DEFAULT '[]', -- JSON array of issued CMDR-XXXX-XXXX-XXXX codes
    quantity INTEGER,                    -- seats purchased, NULL until the mint step lands
    license_type TEXT,                   -- 'commercial_subscription' | 'commercial_perpetual'
    customer_email TEXT,                 -- who the licenses went to (support lookups)
    claimed_at TEXT NOT NULL,            -- ISO 8601, reset when a stale claim is taken over
    issued_at TEXT,                      -- ISO 8601, set once short_codes are stored
    emailed_at TEXT                      -- ISO 8601, set once Resend accepted the email
);
