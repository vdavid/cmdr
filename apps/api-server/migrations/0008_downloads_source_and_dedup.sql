-- Two columns to make the raw download stream cleaner without losing any rows.
--
-- `hashed_ip`: SHA-256(client IP + daily salt), the same per-day-pseudonymous scheme `update_checks`
-- already uses (migration 0002). We keep one row per request (raw count stays `COUNT(*)`), and derive
-- a same-day-deduped count at query time with `COUNT(DISTINCT hashed_ip)`. The salt rotates daily, so
-- the value is not linkable across days and is not reversible to an IP.
--
-- `source`: where the download came from, set by the `/download` handler: 'homebrew' (Homebrew cask,
-- detected by User-Agent), 'website' (getcmdr.com button, carries `?src=website`), or 'other' (shared
-- direct links, etc.). Bot/unfurler hits are dropped before insert, so they never get a row at all.
--
-- Both columns are nullable: rows written before this migration keep NULL, which the dashboard reads
-- as source 'other' and excludes from the unique count.
ALTER TABLE downloads ADD COLUMN hashed_ip TEXT;
ALTER TABLE downloads ADD COLUMN source TEXT;
