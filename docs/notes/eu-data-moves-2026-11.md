# EU data moves (#805), 2026-11

What vdavid/cmdr#805's EU and US items got done on 2026-11-05, what's live, the runbooks for what waits, and the public
passages for David's review. A one-off runbook note: delete it once every cutover below is done and its old resources
are gone.

## Status per item

- **A1, email (Resend → SMTP2Go EU)**: Listmonk is LIVE on SMTP2Go. The API server's code is on this branch and waits
  for one secret (§ A1 runbook). Resend leaves the public pages with this branch.
- **A2, D1 and R2 in the `eu` jurisdiction**: both EU resources exist. The R2 copy is complete and verified. The D1
  switch is rehearsed on a local copy, and blocked on Workers Paid (§ A2 runbook). No binding is switched on this
  branch, so landing it moves nothing.
- **A3, updates from EU R2**: § A3.
- **A4–A6, B1–B3, B5, B7, B8**: public text on this branch, § Changed public passages.
- **B4** (insurance) deferred; **B6** (contract terms) lives in David's vault.

## Infra changes made outside the repo (2026-11-05)

- **SMTP2Go** (getcmdr.com account): SMTP user `listmonk.getcmdr`, created through `/v3/users/smtp/add` with open and
  click tracking, the unsubscribe footer, and archiving off, bounce notifications dropped. Its password is in the sops
  store as `SMTP2GO_LISTMONK_SMTP_PASSWORD`.
- **Listmonk** (`mail.getcmdr.com`, v6.2.0): a new enabled SMTP block `email-smtp2go-eu`, host `mail-eu.smtp2go.com`,
  port 587, STARTTLS, LOGIN. The Resend block is disabled but kept as the rollback; the SES block was already disabled.
  The SMTP test through it reached `david@getcmdr.com`, which mailcow accepts only past DMARC `p=reject`, so From
  alignment holds (SMTP2Go signs with `s812023._domainkey.getcmdr.com`, return path `em812023.getcmdr.com`, both
  verified).
- **Cloudflare R2**: bucket `cmdr-error-reports-eu`, jurisdiction `eu`, holding a copy of all 274 objects of
  `cmdr-error-reports` (27.7 MB). Key set, sizes, etags, and HTTP and custom metadata all match (checked object by
  object with the tool below). Lifecycle: the same 90-day rule on `error-reports/` and the 7-day multipart rule, plus
  one rule per copied date prefix (`copied-prod-<date>`, 55 of them) that deletes it on `<date> + 90 days`. A copy gets
  a fresh upload date, so without those the copies would outlive the privacy policy's 90 days.
- **Cloudflare D1**: database `cmdr-telemetry-eu` (`936d4ece-7f66-45d9-be68-dc1135790bdd`), jurisdiction `eu`, empty.
- **Nothing deleted, nothing switched.** The old bucket and database serve production exactly as before.

## The tool

`apps/api-server/scripts/eu-move/worker.js`, run ONLY through `wrangler dev` with remote bindings
(`scripts/eu-move/wrangler.jsonc`), on 127.0.0.1. ❌ Never deploy it: a public Worker bound to both production databases
(buyer emails) and both buckets would be exposure for nothing. Its header lists the routes. Start it with
`CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` set (the token from the sops store, the account id from
`docs/tooling/cloudflare.md` or the vault's Cloudflare tooling note):

```sh
cd apps/api-server
pnpm exec wrangler dev --config scripts/eu-move/wrangler.jsonc --ip 127.0.0.1 --port <free port> \
  --show-interactive-dev-session=false
curl -s 'http://127.0.0.1:<port>/r2/diff?from=old&to=eu'
```

It never deletes or overwrites. Every route names a direction, so the same calls run a rollback.

## Rehearsal results (2026-11-05)

- `wrangler d1 export cmdr-telemetry --remote`: 10 seconds end to end for 153 MB (an export blocks the database while it
  runs). 128 MB of SQL, holding all 29 `d1_migrations` rows and the `sqlite_sequence` rows.
- Imported into a LOCAL D1: every table's row count and max id matched production, apart from one new `analytics_event`
  and one new `heartbeat` row written after the export. `/d1/reconcile` (old → copy, `apply=1`) moved exactly those two,
  and then `/d1/compare` matched every table's full-row SHA-256 fingerprint against production.
- The real API Worker, run locally on that copy, answered `/validate` identically to production for all four licenses in
  the ledger (one manual, which resolves ONLY from D1, and three Paddle), an unknown manual id, and an unknown `txn_`
  id. No device id, so neither side wrote anything.
- The export and the local copy were deleted afterwards (they hold buyer emails).

## A1 runbook: the API server's email

1. **David**: in the SMTP2Go dashboard (`app-eu.smtp2go.com`, getcmdr.com account) → Sending → API Keys → Add, a key for
   the Worker allowed ONLY `/email/send` and `/stats/email_cycle`, with tracking, the unsubscribe footer, and archiving
   off. The agent key can't create keys (`/api_keys/add` isn't among its endpoints), and it can delete our sending
   domain, so it must not go into the Worker.
2. `pnpm --filter @cmdr/api-server exec wrangler secret put SMTP2GO_API_KEY`, and store the same value in the sops store
   (`SMTP2GO_API_KEY_CMDR_WORKER`). Setting a secret redeploys the CURRENT code with it, so this is safe before the
   land.
3. Land. The deploy workflow's "Check required Worker secrets" step refuses to deploy without the secret, keeping the
   old Worker (and Resend) serving.
4. Verify:
   `curl -s https://eu-api.smtp2go.com/v3/activity/search -H "X-Smtp2go-Api-Key: $(secret SMTP2GO_API_KEY_GETCMDR)" -H 'Content-Type: application/json' -d '{"limit":20}'`
   after the next digest or morning brief (06:30 Stockholm), and the next 00:00 UTC probe stays quiet (no `cron-failure`
   ops event).
5. **A week of deliverability**: daily, `/v3/stats/email_bounces` and `/v3/stats/email_spam`, plus the DMARC reports
   (`dmarc@getcmdr.com`). A license email in spam is a sales problem.
6. **Quota**: the account has 1,000 emails per cycle (14th to 13th), shared by mailcow, Listmonk, and the Worker; 124
   were used by 2026-11-05. The daily probe alarms below 15% left. A newsletter to more than about 600 people needs the
   next plan first.
7. **After the week** (from 2026-11-12, David's call): `wrangler secret delete RESEND_API_KEY` (and the unused
   `PRICE_ID_SUPPORTER`), remove Listmonk's disabled Resend block, delete the Resend DNS records (`send.getcmdr.com` MX
   and TXT, `resend._domainkey.getcmdr.com`), close the Resend account, and drop `RESEND_LISTMONK_SMTP_KEY` from the
   sops store.

Rollback: Listmonk, re-enable the Resend block and disable `email-smtp2go-eu`. The Worker, revert the A1 commit and land
(the Resend secret is still set until step 7).

## A2 runbook, R2 (`cmdr-error-reports` → `cmdr-error-reports-eu`)

Preconditions:

- **David**: add `cmdr-error-reports-eu` to the bucket scope of BOTH R2 API tokens: the Worker's presign token
  (`cmdr-error-reports-presign`, its id is `R2_ACCESS_KEY_ID`) and the report-triage token (`TRIAGE_R2_ACCESS_KEY_ID`).
  Editing the scope keeps the key id and secret, so no secret changes. Keep the old bucket in scope until a day after
  the switch, so links minted before it still open.

The switch commit:

1. `apps/api-server/wrangler.toml`: the `ERROR_REPORTS_BUCKET` binding gets `bucket_name = "cmdr-error-reports-eu"` and
   `jurisdiction = "eu"`; `[vars]` gets `R2_ERROR_REPORTS_BUCKET_NAME = "cmdr-error-reports-eu"` and
   `R2_ERROR_REPORTS_JURISDICTION = "eu"` (`error-report-bucket-config.test.ts` fails if any of the four disagree).
2. `scripts/report-triage/fetch-bundle.sh` (host `${TRIAGE_R2_ACCOUNT_ID}.eu.r2.cloudflarestorage.com`, bucket),
   `scripts/report-triage/settings.json` (`allowedDomains`), and the bucket name in `run.sh`, `DETAILS.md`.
3. `docs/tooling/feedback-and-error-digest.md`: the bucket name in the REST calls, plus the header
   `cf-r2-jurisdiction: eu`. Also `scripts/setup-cf-infra.sh` (`--jurisdiction eu`), `docs/security.md`, and the
   lifecycle command in `src/telemetry/DETAILS.md` (`-J eu`).

Cutover:

1. `pnpm land -no-wait`, wait for green CI, then `pnpm land`. The deploy follows in about five minutes.
2. After it: `/r2/copy?from=old&to=eu&limit=500` for the objects written meanwhile, then `/r2/diff`. A key listed under
   `differing` is an `.amend.json` rewritten in the old bucket after the copy (the tool never overwrites): copy that one
   by hand, old → EU.
3. Add expiry rules for any new date prefix the delta brought (rebuild with the recipe in § Lifecycle recipe).
4. Verify: `/admin/error-reports` lists the bundles; the next digest's download link opens; the triage box fetches a
   bundle.

Rollback: revert the switch commit and land, then `/r2/copy?from=eu&to=old`. Nothing gets deleted from the old bucket
until David decides, at least a week after the switch.

## A2 runbook, D1 (`cmdr-telemetry` → `cmdr-telemetry-eu`)

Preconditions:

- **David: Workers Paid ($5 a month).** Workers Free allows 100,000 rows written a day ACROSS THE ACCOUNT, and past it
  every D1 query errors until 00:00 UTC (D1 pricing docs, checked 2026-11-05). Production already writes 36,000–59,000 a
  day, and the import is about 290,000 rows plus their index rows. On Free it would take the production database down
  for the rest of the day: manual licenses would answer `upstream_error`, and purchases couldn't be claimed.
- With Paid, the D1 size cap rises from 500 MB to 10 GB, so `d1MaxDbBytes` (cron job 7) changes in the same commit, or
  its size alarms fire at the wrong steps.
- A remote dry run first: the same export → import → bump → compare into a scratch database
  (`wrangler d1 create cmdr-telemetry-rehearsal --jurisdiction eu`), then delete the scratch one. It proves the remote
  import and whether D1 accepts `UPDATE sqlite_sequence` (if it refuses with `SQLITE_AUTH`, bump each table by inserting
  a row at `max(id) + 1000000` and deleting it; AUTOINCREMENT keeps the sequence).
- Freeze other api-server lands until step 7: a migration landing in between would reach only the old database, and the
  deploy workflow has no concurrency group.

Steps:

1. The switch commit: `apps/api-server/wrangler.toml` `database_name = "cmdr-telemetry-eu"`,
   `database_id = "936d4ece-7f66-45d9-be68-dc1135790bdd"`, plus the docs that name the database
   (`apps/api-server/DETAILS.md` § Storage, `docs/tooling/cloudflare.md`, `docs/tooling/feedback-and-error-digest.md`).
   The deploy workflow migrates by binding, so it follows. `pnpm land -no-wait`, wait for green CI.
2. Capture production's `/validate` answers for every ledger license (the rehearsal's script, no device id), to compare
   after.
3. Right after a cron tick (`0 */3 * * *` UTC; not near 04:30 or 05:30):
   `wrangler d1 export cmdr-telemetry --remote --output <scratch>/prod.sql`.
4. `wrangler d1 execute cmdr-telemetry-eu --remote --file <scratch>/prod.sql -y`.
5. `/d1/stats?db=eu` (29 migrations), then `/d1/marks?db=eu` (save the JSON: the max ids AT THE EXPORT), then
   `/d1/bump?db=eu&by=1000000`. The bump makes every id the new Worker writes land far above anything the old database
   hands out meanwhile, so gap rows keep their own ids and every later step can rerun safely.
6. `pnpm land`. CI is already green, so it lands at once.
7. When the deploy finishes, wait two minutes (in-flight `waitUntil` writes), confirm the old database's max ids have
   stopped moving (`/d1/stats?db=old`), then `/d1/reconcile?from=old&to=eu&marks=<url-encoded marks>` (dry), then the
   same with `apply=1`. Settle anything under `differing` by hand: those are rows changed in place in the old database
   during the window (a digest stamp, `answered_at`, an ops-event count, a license row).
8. Verify: production `/validate` answers match step 2; `/d1/compare?from=old&to=eu` differs only where the EU side
   moved on; the next cron tick is clean (healthchecks.io, no `cron-failure`), and the morning brief's tripwires are
   quiet.
9. Delete `<scratch>/prod.sql`. Keep the old database untouched (nothing binds it, so nothing writes it) for at least
   seven days. Deleting it is David's call after that.

Rollback: revert the switch commit and land, then `/d1/reconcile?from=eu&to=old` with marks = the OLD database's max ids
at the switch (from step 7's stats). The bump put every EU-written id above them.

## Lifecycle recipe

The per-date expiry rules come from the bucket's date prefixes. With `prefixes.txt` holding one
`error-reports/<env>/<yyyy-mm-dd>/` per line:

```sh
jq -R -s 'split("\n") | map(select(length>0)) | map(. as $p | ($p | capture("/(?<d>[0-9]{4}-[0-9]{2}-[0-9]{2})/$").d) as $d
  | {id: ("copied-" + ($p|ltrimstr("error-reports/")|rtrimstr("/")|gsub("/";"-"))), enabled: true,
     conditions: {prefix: $p},
     deleteObjectsTransition: {condition: {type: "Date", date: (($d + "T00:00:00Z" | fromdateiso8601) + 90*86400 | todateiso8601)}}})' prefixes.txt
```

Append those to the two base rules (read them with `GET .../r2/buckets/cmdr-error-reports-eu/lifecycle` and header
`cf-r2-jurisdiction: eu`) and `PUT` the whole set back: a `PUT` replaces every rule. The rules for dates that have
passed can go any time after 2027-02-03.
