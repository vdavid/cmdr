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
- **A3, updates from EU R2**: the bucket and `download.getcmdr.com` are LIVE with 0.59.0 backfilled; the `/download`
  redirect moves when this lands, and `latest.json` with the next release (§ A3).
- **A4–A6, B1–B3, B5, B7, B8**: public text on this branch, § Changed public passages. B8 is done for everything that's
  true after the land; the "not locked to the EU yet" lines change when the D1 and R2 switches finish (a `DevTodo` on
  `/trust#eu` lists them).
- **A5 list**: Listmonk list 5, "Cmdr subprocessor updates" (public, double opt-in, UUID
  `990aee1b-97df-4a5b-be6f-d4def6783e87`), LIVE; the signup route and form ship with this branch.
- **B4** (insurance) deferred; **B6** (contract terms) lives in David's vault.
- **David's vault**: the DPA draft's Annex IV (Resend out, SMTP2Go in, GitHub's DPA wording, Cloudflare's location) and
  the commitment note's list details are updated in place.

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

## A3, updates from EU R2

The mechanism, caching, and rollback live in `docs/guides/releasing.md` § Where downloads are served from; this is the
2026-11-05 record and the land-time steps.

### What we found

- **Nothing shipped pins GitHub.** The macOS updater fetches whatever HTTPS URL `latest.json` names, in every version
  since the custom updater appeared (`reqwest::get(&url)` in `190a637767`, 2026-03-13; today `fetch_verified_tarball`
  through `server_request::send`, whose only gate is the managed policy). The Tauri updater plugin before it, and on
  Linux, doesn't check the host either. No CSP applies (the download runs in Rust), no capability names a host, and the
  minisign and Developer ID checks aren't tied to GitHub. `cmdr-http`'s proxy and PAC handling routes by host but allows
  any. So a direct switch is safe for every version in the field.
- **Who writes `latest.json`**: the release pipeline's `publish` job, onto the GitHub release; the website deploy copies
  the Latest release's file into `getcmdr.com/latest.json`. So the switch rides the next release: 0.59.0's manifest
  keeps naming GitHub until then.
- **Also pinned to GitHub, now fixed**: the API server's `/download` redirects, the website's direct DMG links, and
  `release-evidence`'s manifest checks (`checkManifest`, `builtInRepo`), which would have failed step 12 of the next
  release.
- **Bug**: the blog posts' download dropdown linked the Intel DMG as `_x86_64.dmg`, which 404s (release files say
  `x64`). Fixed in `d4a778d676`.

### What we built

- `96dae05996`: `scripts/release-mirror.sh`, the `guard` preflight, the `publish` mirror step, mirror URLs in
  `latest.json`, `release-evidence` accepting them, and the docs.
- `da93ca6a2c`: the `/download` redirect sends 0.59.0 and later to the mirror, older to GitHub.
- `d4a778d676`: website links and the `/trust` passages below.

### Infra created (2026-11-05)

- **R2 bucket `cmdr-releases-eu`**, jurisdiction `eu` (location `EEUR`), Standard class.
- **Custom domain `download.getcmdr.com`** on it (zone getcmdr.com, min TLS 1.2), created through the API with the sops
  token: no dashboard step needed. No DNS record to add; R2 manages it.
- **Backfill of v0.59.0**: the 11 mirrored files (three archives, three `.sig`, three DMGs, the pkg, `checksums.txt`),
  each checked against GitHub's digest and `checksums.txt`, then read back through the public host with matching
  SHA-256. `.dmg` and `.gz` come back as edge-cache `HIT`; `.sig`, `.txt`, and `.pkg` aren't edge-cached by Cloudflare's
  default rules (cheap; a "cache everything" rule for the host is optional and would mean editing the zone's existing
  cache ruleset).
- `preflight/last-check.txt` exists from the local preflight run.
- Nothing deleted.

### Public passages changed in `apps/website/src/lib/trust.ts`

- `updates` destination: "`api.getcmdr.com`, then `getcmdr.com/latest.json`. The update itself downloads from
  `download.getcmdr.com`, which stores the files in the EU. An update to 0.59.0 or older downloads from `github.com` and
  `release-assets.githubusercontent.com` instead, which stay a public mirror of every release."
- `allowlistHosts`: new `download.getcmdr.com`: "update downloads"; `github.com, release-assets.githubusercontent.com`:
  "update downloads while 0.59.0 is the newest release (later ones come from download.getcmdr.com)".
- Once a release after 0.59.0 is Latest and nobody would roll back to 0.59.0, the GitHub allowlist line can go.

### Runbook

1. **David, before the next release**: make sure the GitHub repo secret `CLOUDFLARE_API_TOKEN` can write R2. It was set
   2026-03-21 and its value can't be read, so whether it's the same token as sops' (which can, verified by the
   preflight) is unknown. A hint that it can: the API server's CI deploy already passes wrangler's R2 bucket precheck,
   which needs "Workers R2 Storage: Edit" (`apps/api-server/DETAILS.md` § R2 presigned URLs). Check: after landing, the
   next release's `guard` job fails at "Require write access to the EU download bucket" if it can't. Fix either way:
   dashboard → My Profile → API Tokens → that token → add "Workers R2 Storage: Edit" (no value change), or, better for
   least privilege, create a token with only "Workers R2 Storage: Edit" on the account, store it as a new secret, and
   point the two workflow steps at it (a one-line change each).
2. **Land time (lead)**: nothing extra. The API server and website deploy with `main`; `/download` starts sending 0.59.0
   to the mirror at once (backfilled), and `latest.json` moves at the next release.
3. **Next release**: run as usual. `publish` mirrors before marking Latest.

### How to verify after the next release

- `curl -s https://getcmdr.com/latest.json | jq -r '.platforms[].url'` prints three
  `https://download.getcmdr.com/v<new>/` URLs, and `./scripts/release-evidence.sh verify <new>` passes (it probes each
  one).
- The `publish` log shows "Verified ..." for 11 files (10 without a pkg).
- `curl -sI https://api.getcmdr.com/download/latest/universal` redirects to `download.getcmdr.com`.
- An installed 0.59.0 updates through Cmdr > Check for updates (its log line "Downloading update ... from
  https://download.getcmdr.com/...").

### Rollback

- **Mirror broken, installs can't update**: upload a `latest.json` naming the same release's GitHub URLs to that release
  (`gh release upload vX.Y.Z latest.json --clobber`) and re-run the website deploy; revert the `BASE_URL` line in
  `publish` before the next release.
- **Download redirect**: raise `firstMirroredVersion` past the latest version (or revert `da93ca6a2c`) and land.
- The bucket and domain can stay; deleting them is David's call.

## Changed public passages (for David's review)

Everything below ships as a v0 when this branch lands. Your #805 comment asked to see public text before it ships, and
the lead's brief said v0 at the land with your review after; whoever lands it picks. The exact text is in the diff of
`e7f78a885a` (and `d4a778d676`, `a2a902bac3` for the A3 and signup passages).

`/trust` (`apps/website/src/pages/trust.astro`, `apps/website/src/lib/trust.ts`):

- `#eu`, "Does data leave the EU?": now "Some of it does", with the list rewritten (downloads and updates, GitHub
  reports under DPF with no DPA, every email through SMTP2Go's EU servers, Cloudflare's nearest-data-center request
  handling and the not-yet-locked storage, Regional Services not used, Paddle's UK adequacy, and the A6 line "Cmdr's
  on-device AI keeps everything on the Mac").
- `#outside-eu`, new: "Where is my data? For reviewers outside the EU" (B1).
- `#no-soc2`, new, under Compliance: "No SOC 2 yet, and what you get instead", with the questionnaire offer for 50 seats
  or more (B3). The Known limitations line about audits links to it.
- `#subprocessors`: Resend removed; SMTP2Go "Sends every email Cmdr sends..."; Cloudflare's "What" adds the release
  files and its "Where" says US company, EU jurisdiction for release files, Eastern Europe placement for the rest,
  nearest data center, DPF plus SCCs; GitHub's "What" says public mirror and its "Where" states DPF and why there's no
  DPA (A4).
- `#subprocessor-changes`, new: "When we change who handles your data", the vault's public wording almost word for word,
  plus the signup form and the dated change log (two 2026-11-05 entries: Resend removed, downloads moved) (A5).
- `#company`: the DPA bullet says "30 days' notice before a change to the subprocessors" and links the new section; a
  new Support bullet carries B7's reply times. The old "There's no written response time" sentence is gone.
- `#signing`: downloads come "from `download.getcmdr.com` or its mirror on GitHub Releases".
- Known limitations: "Cloudflare's database and error-report storage aren't locked to the EU jurisdiction yet."
- `verifiedAgainst.date`: 2026-11-05 (version stays 0.59.0).
- A3's `updates` connection and allowlist lines: § A3 above.

Privacy policy (`apps/website/src/pages/privacy-policy.astro`, last updated 2026-11-05):

- §2 Downloads: the file comes from `download.getcmdr.com`, so Cloudflare sees the IP address.
- §5: the Cloudflare bullet adds `download.getcmdr.com` and "Cloudflare is a US company"; GitHub becomes "release mirror
  and issue tracking", with the controller and no-DPA sentence; the Resend bullet is gone; SMTP2Go is "all outgoing
  email" via its EU servers; Listmonk's "except when sending emails via SMTP2Go", and the subprocessor updates list (the
  signup commit).
- §6: where each part is stored and the transfer basis (DPF, SCCs, New Zealand's and the UK's adequacy).
- §10, new: "Your rights in the US (California and other states)" (B2). The old §10–12 are now §11–13.

Pricing FAQ (`apps/website/src/pages/pricing.astro`):

- "Who sells it, and which tax form does our accounts payable team need?" (B5). It links Paddle's buyer support
  (paddle.net), because no official Paddle page offering its W-8BEN-E turned up; a `DevTodo` beside it says so.
- "When does support answer?" (B7).

## Open items for David

1. **SMTP2Go key for the Worker** (A1 runbook step 1), then `wrangler secret put SMTP2GO_API_KEY`. Until then, every
   api-server deploy stops at the secrets check, and the old Worker keeps sending through Resend.
2. **Workers Paid** ($5 a month) before the D1 switch. It also lifts the 500 MB cap the size alarm tracks.
3. **The two R2 token scopes** (A2 runbook, R2 preconditions) before the R2 switch.
4. **The GitHub `CLOUDFLARE_API_TOKEN`'s R2 write access** before the next release (§ A3 runbook step 1).
5. **Paddle's W-8BEN-E link**: confirm the form and find an official page for it, or ask Paddle support.
6. **Review the public passages** above, and the support-hours wording: "Stockholm time (CET)" is CEST in summer;
   "Stockholm time" alone would always be right.
7. **After the week** (from 2026-11-12): the Resend cleanup in A1 step 7, and later deleting the old D1 database and R2
   bucket once their switches have run a week.
8. **SMTP2Go quota**: 1,000 emails a cycle across mailcow, Listmonk, and the Worker. The lists are small today (26 and
   18 subscribers), but a bigger newsletter needs the next plan.
9. **Listmonk's public form** (`getcmdr.com/subscription/form`) now shows the subprocessor list too, pre-ticked with the
   newsletter and beta lists, as Listmonk does for every public list. The `/trust` form subscribes to that list alone.
