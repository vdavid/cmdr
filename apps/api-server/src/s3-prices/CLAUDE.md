# S3 price table

`GET /s3-prices/v1` (`s3-prices.ts`): the S3 list-price table the desktop app prices a planned copy, move, or delete
with. The app bundles the same file as its fallback and fetches this one, so a price change reaches every install with a
Worker deploy. No D1, no KV, no secrets, no IP touch.

## Must-knows

- **`s3-prices.json` is byte-identical to `crates/cmdr-s3/src/cost/s3-prices.json`**, enforced by `s3-prices.test.ts`
  (node project, it reads both files). A price edit changes the crate file, then `cp`s it here. ❌ Never reformat this
  copy (`apps/api-server/.prettierignore` lists it for that reason).
- **The path's `v1` is the table's `schemaVersion`.** A breaking schema change serves at `/s3-prices/v2` beside v1,
  since shipped apps keep fetching v1 for as long as they run.

- **A monthly claude.ai routine watches the prices** ("S3 price table watch (monthly)", the 3rd of each month, 07:17
  UTC). It reads each provider's `source` page on `origin/main`'s table: no change means a one-line report and nothing
  else, while a change opens a `claude-created` PR updating both copies. Keep each entry's `source` pointing at the
  official pricing page, since that's what it reads.

Response headers, caching, and why there's no rate limit: `DETAILS.md`.
