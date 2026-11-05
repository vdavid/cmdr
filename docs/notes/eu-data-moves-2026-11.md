# EU data moves (#805), 2026-11

What vdavid/cmdr#805's EU and US items got done on 2026-11-05, what's live, the runbooks for what waits, and the public
passages for David's review. A one-off runbook note: delete it once every cutover below is done and its old resources
are gone.

## Status per item

- **A1, email (Resend → SMTP2Go EU)**: LIVE. Listmonk since 2026-11-05 morning; the API server since David set
  `SMTP2GO_API_KEY` and deploy run 42025533993 went green (2026-11-05 12:15 UTC, the Worker now holds the secret). The
  first Worker send shows in SMTP2Go's activity at the next digest or the 06:30 brief (§ A1 runbook step 4).
- **A2, D1 and R2 in the `eu` jurisdiction**: the switch commit, the cutover script, and a full remote rehearsal are on
  the branch; it waits for the R2 token scopes and the lead's land (§ A2 cutover).
- **A3, updates from EU R2**: the bucket and `download.getcmdr.com` are LIVE with 0.59.0 backfilled; the `/download`
  redirect moves when this lands, and `latest.json` with the next release (§ A3).
- **A4–A6, B1–B3, B5, B7, B8**: public text LIVE on `main`, § Changed public passages. The switch commit replaces the
  "not locked to the EU yet" lines with "stored in Cloudflare's EU jurisdiction" (naming the KV store of license
  activation codes as the part that stays global), drops that Known limitations clause, and adds a dated Cloudflare line
  to the change log. B5: a second search (2026-11-05) found no official Paddle page with its W-8BEN-E, so the FAQ asks
  buyers to request it from Paddle's buyer support, and its `DevTodo` is gone.
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

## A2 cutover (D1 `cmdr-telemetry` → `cmdr-telemetry-eu`, R2 `cmdr-error-reports` → `cmdr-error-reports-eu`)

Both switch in ONE land: the switch commit changes the two bindings (`apps/api-server/wrangler.toml`), the presign vars,
the triage box's host and allowlist (`scripts/report-triage/`), the docs that name either resource, and the public text
(`/trust`, privacy policy, a change-log line). `apps/api-server/scripts/eu-move/cutover.sh` drives the land-time steps;
its gates refuse to run a phase out of order.

### Done before the land (2026-11-05)

- Workers Paid is on (David), which lifted the account-wide 100,000 rows a day that would have taken production down
  mid-import. `D1_MAX_DB_MB` (`wrangler.toml` `[vars]`) follows it to 10 GB.
- **Plan review** (a separate agent): two blockers, both fixed. (1) Since per-lane CI, `pnpm land -no-wait` runs a light
  CI that never reports `CI OK`, so a `pnpm land` after the import would start a 25–35-minute run inside the window; the
  sequence now gets a green land run FIRST, and `prepare` refuses without one. (2) The reconcile only reported keyed
  rows, so a license minted, refunded, or claimed in the window would have stayed behind (and a Paddle redelivery could
  mint a second set); it now copies them, carries forward-only stamps so the EU cron doesn't re-file cards or re-send
  digests, counts real writes, and the bump covers a table with no `sqlite_sequence` row.
- **Remote rehearsal** of the whole script against a scratch EU database (`cmdr-telemetry-rehearsal`, deleted after):
  export 10 s, import 24 s (299,095 changes, 1.88 M rows written with indexes, 148 MB), 29 migrations, D1 accepts
  `UPDATE sqlite_sequence`, a new row took id `max + 1000000`, the reconcile moved one window row, and the `/validate`
  snapshot (four licenses plus two unknown ids) matched. The first `wrangler d1 export` of the day failed with no
  message and the rerun worked, so `prepare` retries it three times.
- R2: the delta copy found nothing new (274 objects both sides). The EU bucket's lifecycle has
  `expire-error-reports-90-days` plus the per-date rules.

### Land-time sequence (the lead)

Preconditions: David has added `cmdr-error-reports-eu` to both R2 tokens' bucket scope (§ Open items 3), and this
command answers `200` (the triage token; the presign one can't be checked from here):
`curl -s -o /dev/null -w '%{http_code}\n' --aws-sigv4 aws:amz:auto:s3 --user "$(secret TRIAGE_R2_ACCESS_KEY_ID):$(secret TRIAGE_R2_SECRET_ACCESS_KEY)" "https://6a4433bf11c3cf86feda057f76f47991.eu.r2.cloudflarestorage.com/cmdr-error-reports-eu?list-type=2&max-keys=1"`.
No other api-server land until step 6 is done.

1. From the worktree: `pnpm land -no-wait`, then `gh pr edit --add-label land` and wait for that land run's `CI OK`
   (25–35 min). Nothing has moved yet. (Rebase first if `main` has moved far, so the merge in step 4 stays trivial.)
2. `D=<scratch>/cutover; apps/api-server/scripts/eu-move/cutover.sh snapshot $D/before.json`.
3. Not within 15 minutes before a cron tick (`0 */3` UTC, plus 04:30 and 05:30), so no EU tick runs before step 5:
   `apps/api-server/scripts/eu-move/cutover.sh prepare $D` (about a minute; production's database is unavailable for
   about 10 s of it). It's resumable: if it stops after the import, rerun it (the `imported` marker skips to the marks
   and the bump). If the import itself failed, `cmdr-telemetry-eu` may be half-filled: stop, nothing has switched, and
   ask for a fresh EU database (its id goes into the switch commit).
4. `pnpm land -merge` (in the background): it lands the green head at once, as a merge commit if `main` has moved. ❌
   Never a plain `pnpm land` here: it rebases onto a moved `main`, and the new head needs a fresh 25–35-minute CI run
   inside the window. `deploy-api-server.yml` deploys about five minutes later, and the website deploys the public text
   from the same push.
5. `apps/api-server/scripts/eu-move/cutover.sh finish $D`: waits for the deploy (the Worker's `TELEMETRY_DB` binding),
   then for the old database to go quiet, reconciles, copies the window's R2 objects, and compares `/validate` for every
   license in the snapshot (a license sold in the window is listed as new). It counts only what needs a human: `ignored`
   rows on `update_checks`, `analytics_event`, `daily_active_users`, and `downloads_daily_unique` are UNIQUE duplicates
   the EU side already counted, and `cron_job_streaks` / `db_size_samples` rows are the cron's own bookkeeping.
6. Verify: the next cron tick is clean (healthchecks.io, no `cron-failure` ops event), the next digest's bundle link
   opens, and re-install the triage box (`m1mbp/ansible`, `--tags report_triage`, from the laptop's updated checkout).
   Delete `$D` (it holds license ids).
7. If the land happens after 2026-11-05, fix the date of the Cloudflare line in `subprocessorChanges` (`trust.ts`).

Write window: from the export in step 3 to the deploy in step 4, about 6–8 minutes (the second review caught that a
plain `pnpm land` would stretch it past 30). Nothing pauses: writes in it land in the old database and step 5 carries
them over.

Rollback: revert the switch commit and land. Then, with the tool, `/d1/reconcile?from=eu&to=old&marks=<$D/marks.json>`
(dry, then `apply=1`): rows the EU side wrote sit above the marks (the bump), the window rows the old side already has
are skipped, and keyed rows and stamps carry over as in step 5 (the stamp pass reads whole tables, so the EU cron's
stamps on window rows go back too, and the old cron doesn't re-file those cards). Then `/r2/copy?from=eu&to=old`. The
old database and bucket stay untouched for at least seven days after the switch; deleting them is David's call (not
before 2026-11-12).

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
- Known limitations: "Cloudflare's database and error-report storage aren't locked to the EU jurisdiction yet." The A2
  switch commit drops that clause, and rewrites `#eu`, `#outside-eu`, the Cloudflare subprocessor entry, and privacy
  policy §6 to "stored in Cloudflare's EU jurisdiction", naming the KV store (license activation codes) as global.
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
  (paddle.net) and asks buyers to request the form there, because no official Paddle page offering its W-8BEN-E turned
  up in two searches (2026-11-05).
- "When does support answer?" (B7).

## Open items for David

1. Done 2026-11-05: the Worker's SMTP2Go key, and Workers Paid.
2. Done 2026-11-05: Paddle's W-8BEN-E (no official page exists; the FAQ points to buyer support).
3. **The two R2 token scopes**, before the A2 land. No token in the sops store can read or edit API tokens
   (`/user/tokens` and `/accounts/<id>/tokens` answer 9109, checked 2026-11-05), so it's a dashboard edit: R2 Object
   Storage → Manage API tokens → `cmdr-error-reports-presign` (its Access Key ID is the Worker's `R2_ACCESS_KEY_ID`) →
   Edit → under the bucket scope, add `cmdr-error-reports-eu` (it's listed under the EU jurisdiction) and keep
   `cmdr-error-reports` → Save. The same for the report-triage token (Access Key ID starting `1d0073`). The key id and
   secret stay the same. Today the triage token gets `403` on the EU bucket and `200` on the old one.
4. **The GitHub `CLOUDFLARE_API_TOKEN`'s R2 write access**, before the next release (§ A3 runbook step 1). It can't be
   confirmed from here: no token can list tokens, GitHub won't show a secret's value, and checking it means running a
   workflow. In the dashboard (My Profile → API Tokens), the token whose "last used" matches the 2026-11-05 12:15 UTC
   deploy is the CI one; give it "Workers R2 Storage: Edit" if it lacks it. Otherwise the next release's `guard` job
   stops at "Require write access to the EU download bucket", before anything ships.
5. **Review the public passages** above, and the support-hours wording: "Stockholm time (CET)" is CEST in summer;
   "Stockholm time" alone would always be right.
6. **After the week** (from 2026-11-12): the Resend cleanup in A1 step 7, and later deleting the old D1 database and R2
   bucket once their switches have run a week.
7. **SMTP2Go quota**: 1,000 emails a cycle across mailcow, Listmonk, and the Worker. The lists are small today (26 and
   18 subscribers), but a bigger newsletter needs the next plan.
8. **Listmonk's public form** (`getcmdr.com/subscription/form`) now shows the subprocessor list too, pre-ticked with the
   newsletter and beta lists, as Listmonk does for every public list. The `/trust` form subscribes to that list alone.
