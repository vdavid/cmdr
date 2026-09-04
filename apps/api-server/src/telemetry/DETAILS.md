# Telemetry details

Pull-tier docs for `src/telemetry/`. Must-know invariants live in `CLAUDE.md`; the retention windows every table here
promises, the KV/R2 bindings, and the cron sweeps live in `../../DETAILS.md`.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`telemetry.ts`**: routes `/crash-report`, `/heartbeat`, `/update-check/:version`, `/download/:version/:arch`, plus
  `extractTopFunction`, `validateOptionalEnum` / `validateOptionalPattern`, and the sanitizers.
- **`error-report.ts`**: `POST /error-report` (multipart upload to R2, the KV index write, presigned Discord
  notification, and an email for hand-written reports).
- **`error-report-amend.ts`**: `POST /error-report/:id/amend`, plus the `report:{id}` KV index and the amend credential
  both routes share.
- **`error-report-intake.ts`**: admission control — daily byte budget, intake pause flag, once-a-day alert claims,
  notification fan-out cap.
- **`error-report-eviction.ts`**: 8/6 GB watermarks, the 60-day age floor, the KV lock, `extractDateSegment`, and the
  recompute helper.
- **`feedback.ts`**: `POST /feedback` (in-app feedback → D1 + Discord).
- Tests: `crash-report.test.ts` (incl. § "top_function derivation" over real backtraces), `heartbeat.test.ts`,
  `download-and-update-check.test.ts`, `error-report.test.ts`, `error-report-intake.test.ts`,
  `error-report-eviction.test.ts`, `error-report-email.test.ts`, `error-report-amend.test.ts`, `feedback.test.ts`. The
  route-level R2/KV/D1 fakes live in `error-report-test-helpers.ts`; the Resend mock can't (`vi.mock` is hoisted per
  file).

## Request flows

```
Crash report: POST /crash-report → rate-limit by IP (CRASH_REPORT_LIMITER, 429 if over) → validate payload (size + required fields + optional buildMode/appFate membership + diagId/email shape) → hash IP with daily salt → write to D1 incl. nullable diag_id + email + app_fate (fire-and-forget via waitUntil) → 204

Error report: POST /error-report → rate-limit by IP (ERROR_REPORT_LIMITER, 429 if over) → global intake gates (intake_paused, then the day's byte budget; 503 + Retry-After if either trips, plus one Discord ping the day the budget runs out) → read the body under MAX_BODY_BYTES, cancelling past it (413) → parse multipart, validate bundle + meta (400/413) → stream the bundle to R2 under error-reports/{prod|dev}/{date}/{id}-{uuid}.zip → AWAITED: mint an amend key and write report:{id} to KV → in waitUntil: bump total_bytes, charge the daily budget, tryEvict, a Discord embed (capped at DAILY_NOTIFICATION_CAP/day), then for kind: user only, a notification email (capped at DAILY_ERROR_REPORT_EMAIL_CAP/day) → 200 {id, amendKey}

Error report amendment: POST /error-report/:id/amend → rate-limit by IP (ERROR_REPORT_AMEND_LIMITER, 429 if over) → validate :id shape (400) → read the JSON body under 512 KB (400/413) → validate amendKey + note/email, at least one of the two (400) → read report:{id} from KV (404 if gone) → SHA-256 the presented key, constantTimeEqual against amendKeyHash (401) → read-modify-write the {bundle}.amend.json sidecar in R2 → in waitUntil: an amendment email on the shared DAILY_ERROR_REPORT_EMAIL_CAP allowance → 200 {id, amendments}

Heartbeat: POST /heartbeat → rate-limit by IP (HEARTBEAT_LIMITER, 429 if over) → validate payload (size + required fields + analId/version shape + config-size cap) → write to D1 heartbeat (fire-and-forget via waitUntil), no IP stored → 204

Feedback: POST /feedback → rate-limit by IP (FEEDBACK_LIMITER, 429 if over) → validate shape (required feedback text ≤ 100k code points + appVersion/osVersion, optional email/buildMode) → AWAITED D1 write to `feedback` (failure → soft 502 so the app offers a retry) → Discord ping in waitUntil (DISCORD_FEEDBACK_WEBHOOK_URL, falls back to DISCORD_WEBHOOK_URL) → 204 → the 3-hourly cron mails the row in the feedback digest (`../../DETAILS.md` § Cron handler)

Download redirect: GET /download/:version/:arch → write to D1 (fire-and-forget) → 302 to GitHub Releases

Update check proxy: GET /update-check/:version → hash IP with daily salt → INSERT OR IGNORE into D1 (fire-and-forget) → 302 to latest.json
```

## Crash reports

D1 table `crash_reports`. Columns: `hashed_ip`, `app_version`, `os_version`, `arch`, `signal`, `top_function`,
`backtrace`, `build_mode` (`'release'` / `'debug'`, nullable for legacy rows), `short_id` (`CRASH-XXXXX`, nullable for
legacy rows), `diag_id` (`diag_<uuid>`, nullable), `email` (nullable), `panic_message` (nullable: signal crashes carry
no panic payload, and legacy rows predate the column), `app_fate` (nullable, migration `0015`). Validates payload size
(max 64 KB), required fields, and the shape of optional fields before writing. `diagId` must match
`^diag_[0-9a-f-]{36}$` (a malformed value, including any `anal_`-prefixed one, is rejected 400); `email` is loosely
shape-checked and surfaced as the "Reply to" column in the crash-notification email (`../scheduled.ts` /
`../email/crash.ts`). No authentication required.

**`app_fate` is what ranks a report's severity:** `'ended'` (the app went down with it) versus `'keptRunning'` (a
background-thread panic it survived), plus `'unknown'` / `'unconfirmed'`, which claim nothing. Without it a real crash
and a problem the app walked away from are the same row, since both read `signal: panic`. Validated against the exact
four values the client's `AppFate` enum serializes (`apps/desktop/src-tauri/src/crash_reporter/mod.rs`) and rejected 400
outside them, because the column exists to be grouped on and an invented value would become its own bucket in the
nightly email. Absent or `null` stores NULL, so a client older than the field still reports. The one consumer is the
email's Fate column and subject (`../../DETAILS.md` § Cron handler); `/admin/crashes` groups by day/site/signal and does
not read it.

**`top_function` derivation (`extractTopFunction`):** the grouping key is the topmost backtrace frame that is real
application code. Frames belonging to the panic machinery are skipped first (`crash_reporter`, `std::panicking`,
`core::panicking`, `rust_begin_unwind`, `std::backtrace` / `std::sys::backtrace`, `core::str::slice_error_fail`, and the
`core::option` / `core::result` unwrap-and-expect helpers), then the first `cmdr::` / `cmdr_lib::` frame wins. Without
the skip list every panic grouped under `cmdr_lib::crash_reporter::install_panic_hook::{{closure}}` (the hook is itself
app code and always the first `cmdr` frame), which collapsed 15 of 17 real reports into one bucket and made the nightly
email useless. Non-app frames are never the key even when they are the immediate cause, since
`tokio::task::spawn::spawn` or `core::str::slice_error_fail` would group unrelated bugs by a shared library call; a
backtrace with no app frame stays `'unknown'`.

## Heartbeat

D1 table `heartbeat`. The desktop app posts one beat at launch and hourly for true daily-active tracking during the open
beta. Identity is the random `anal_<uuid>` analytics id (regex `^anal_[0-9a-f-]{36}$`); the IP keys the rate limiter and
is never stored. Required: `analId`, `appVersion` (semver), `osVersion`, `arch`. Optional: `buildMode` and `config`, an
arbitrary object stored verbatim as `config_json`. The config is a single JSON blob, not per-field columns, so new
settings absorb without a migration: DAU/engagement queries never touch it (richer config-shape filtering lives in
PostHog person properties). Caps: 32 KB whole body, 16 KB config blob. No UNIQUE/dedup constraint: every beat is kept
(engagement = beats/day), and DAU (`COUNT(DISTINCT anal_id)`) is computed at query time by `/admin/heartbeat-dau`.

**Rate limiting:** `HEARTBEAT_LIMITER` (`[[ratelimits]]` in `wrangler.toml`, type `RateLimit`, `.limit({ key })` →
`{ success }`) keyed by `cf-connecting-ip` at 12 req/min/IP (`period` must be 10 or 60). Legit traffic is ~1
beat/hour/install, so the cap stops a bloat-spam loop without touching real users; over the limit returns 429 before any
parsing or D1 write. The binding is typed optional so tests and incomplete envs can omit it (the gate is then a no-op).

## Download tracking

D1 table `downloads`. One row per download event with `app_version`, `arch`, `country`, `continent`, `hashed_ip`,
`source`, `ref`, `referer`, `user_agent`, and `ua_family`. The D1 write is fire-and-forget via `waitUntil` +
`.catch(() => {})`. What makes the count meaningful as an install signal:

- **`latest` is a valid `:version`,** for links we can't edit per release (app directories, the README, blog posts, a
  chat message from last year). `resolveLatestVersion` reads `getcmdr.com/latest.json` (the same manifest the in-app
  updater reads, so `latest` can never name a version the updater doesn't know), falling back to GitHub's
  `releases/latest` API for the window where the website is down or mid-deploy. Both answers are validated against
  `versionPattern` before they reach a redirect URL or a D1 row, and both fetches are edge-cached for five minutes, so a
  download burst costs one origin fetch. D1 stores the RESOLVED version, never `latest`. When neither source answers,
  the handler 302s to the GitHub releases page and writes NO row. `getcmdr.com/download/latest/<arch>` is the public
  face of this, an nginx redirect in `apps/website/nginx.conf`.
- **Bot/unfurler hits are dropped:** link-preview bots (Discord, Slack, etc.) and crawlers would inflate the count, so a
  User-Agent denylist skips the D1 write (the 302 is still served). A missing UA is treated as a bot too. Homebrew
  downloads via curl, which would match the `curl` rule, so Homebrew is explicitly exempted.
- **`hashed_ip` enables same-day dedup:** `SHA-256(IP_HASH_PEPPER + IP + daily salt)`, the same scheme as
  `update_checks`. One row per request (raw count is `COUNT(*)`); the dashboard derives distinct same-day downloaders
  with `COUNT(DISTINCT hashed_ip)`. Both ingredients are load-bearing, and the pepper rule is app-wide
  (`../../CLAUDE.md`): the daily salt stops the value linking a visitor across days, and the pepper is what makes it
  one-way at all.
- **`source` tags origin:** `homebrew` (by User-Agent), `website` (the getcmdr.com button, which sends `?src=website`),
  or `other`. In-app auto-updates never appear here: they fetch the tarball straight from GitHub.
- **`ref` tags the first-touch channel** (migration `0009`): where a website visitor originally arrived from (a UTM
  source/campaign, or an external referrer hostname), so the dashboard can attribute installs to a channel. The website
  computes it client-side from URL state only (no localStorage/cookie, to stay banner-free) and forwards it as `?ref=`.
  The handler never trusts that input: `sanitizeRef` lowercases, drops anything outside `[a-z0-9._:-]`, and caps at 120
  chars, mirroring the website's normalization. Absent or sanitizes-to-empty → stored NULL (not `''`). Homebrew, direct
  links, and return visits in a later session carry no ref and stay NULL. The charset rule is the trust boundary; the
  matching admin-side sanitizer lives in `../website/link-codes.ts`.
- **`referer` and `user_agent` capture the hit's own HTTP metadata** (migration `0010`), the first-party signal that
  illuminates the large `(none)` `ref` bucket: a DIRECT hit to `/download` (a link shared on AlternativeTo, a directory,
  GitHub, Reddit, a forum) carries no `ref` yet arrives with a `Referer` naming the page that linked it. Unlike `ref`
  this is NOT client-supplied attribution, so there's no website-side sanitizer to keep in sync. `sanitizeRefererHost`
  keeps the HOST only (never path or query), lowercases, strips a leading `www.`, drops anything outside `[a-z0-9.-]`,
  and caps at 120 chars; absent/unparseable/empty → NULL. `user_agent` is the raw UA capped at 400 chars. Both sit
  beside the daily-rotating `hashed_ip`, so neither adds a cross-day identifier.

**User-Agent family classification (`../user-agent.ts`):** the raw download count over-reads as an install signal
because a large share of `/download` hits are scrapers and non-macOS clients. Cmdr is macOS-only, which is the whole
basis: a Windows/Android/Linux/X11 client fetching the `.dmg` literally cannot install it.

- **`human`** (a possible install, checked first so a Mac-claiming UA is never excluded): UA contains `Macintosh` or
  `Mac OS`, `Homebrew`, or `curl`/`wget`.
- **`bot`** (the one high-confidence exclusion): UA contains `Windows`, `Android`, `Linux`, or `X11`.
- **`unknown`**: anything else, including a NULL UA on rows captured before `user_agent` existed. NEVER excluded.
- **Computed at WRITE time** into `downloads.ua_family` (migration `0013`), so the signal outlives the raw UA that the
  retention sweep clears after 90 days. The read side (`../admin/funnel.ts`) calls `resolveUaFamily`, which prefers the
  stored value and falls back to the classifier for pre-`0013` rows. `classifyUaFamily` stays the single pure definition
  of the rules; the 90-day UA window is what still lets us re-tune it against real UAs. The derived `humanInstalls`
  headline is `human + unknown`, deliberately conservative — and the scraper spoofs Mac UAs, so `human` is NOT a clean
  count. We never exclude by country. Aggregation: `../admin/DETAILS.md` § "Per-day funnel".

## Update check tracking

D1 table `update_checks`. Counts active users (free + licensed) by proxying update checks through
`GET /update-check/:version`; without it there's no signal for how many people actually run the app (website analytics
only track visitors, and download tracking only captures installs). Each unique (date, hashed_ip, app_version, arch)
combo gets one row (`INSERT OR IGNORE` with a UNIQUE constraint handles dedup for free). The IP goes through the same
peppered `hashCallerIp` as `/download`. The cron handler aggregates raw rows into `daily_active_users` daily and prunes
the raw rows at seven days.

## Error reports

**R2 key shape:** `error-reports/{prod|dev}/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`. The env segment (`prod` for release
builds, `dev` for debug, inferred from `meta.buildMode`) keeps dev-run reports out of the production sort order. Legacy
keys (`error-reports/{yyyy-mm-dd}/...`) still exist; eviction reads the date segment via `extractDateSegment`, which
handles both shapes. The 90-day R2 lifecycle drains the legacy shape naturally, so no migration is needed. An amended
report also carries a `.amend.json` sidecar on the same key (§ Amendments).

**`meta.email`:** the reply-to address, present only when the person ticked "Attach my email" in the send dialog. The
auto-dispatcher never sets it (a user who enabled auto-send hasn't consented to shipping their address on every report),
so it is structurally absent from `kind: 'auto'` bundles. Validated with the shared loose `hasEmailShape`
(`../types.ts`), the reply-to form every channel uses, never the stricter licensing `isValidEmail`. It reaches the
server inside the manifest, is stored in the bundle zip, and drives the notification email's `Reply-To`. Its retention
is the bundle's: 90 days, via the R2 lifecycle.

**Body cap:** `content-length` is advisory (a chunked upload declares no length; a declared one can lie), so
`readReportUpload` reads the body itself through `readCappedBody` (`../types.ts`) and cancels past `MAX_BODY_BYTES`
(bundle cap + 1 MB for the `meta` part and multipart framing, sized against the client's 100,000-char `userNote` limit).
Without it the multipart parser would buffer up to Cloudflare's 100 MB request limit inside a 128 MB isolate. Over-cap
returns null rather than throwing, so 413 stays distinguishable from a malformed-multipart 400 without matching on
parser text. The amend route reads its JSON body the same way, under its own 512 KB cap: `c.req.text()` has exactly the
`parseBody` problem, and counting bytes rather than `String.length` keeps the budget from running ~1.5x loose on
multi-byte text.

**Discord notifications:** every upload triggers an embed with a 7-day presigned R2 GET URL, minted through the R2
S3-compatible API via `aws4fetch` (`AwsClient.sign` with `signQuery: true` + `X-Amz-Expires`; 7 days is R2's max).
Click-to-download convenience outweighs leak risk because only the maintainer reads `#error-reports`. The three R2
secrets and their rotation runbook: `../../DETAILS.md` § R2 presigned URLs. `createPresigner` memoizes the URL per
upload, so the email and the embed hand out the same link and an upload that notifies neither signs nothing.

### The report index and the amend credential

**What it is:** one `ERROR_REPORT_META` key per upload, `report:{ERR-XXXXX}` → `{ env, date, key, amendKeyHash }`, where
`key` is the full R2 object key. 90-day TTL, matching the bucket lifecycle, so the index never outlives the bundle it
points at and never survives as a pointer to something already gone.

**Why it exists:** the date is part of the R2 key and nothing else maps an id to one, so finding one bundle used to mean
walking backwards a day per API request. The index makes a report addressable by the id a person can actually read: one
KV get for triage (`docs/tooling/feedback-and-error-digest.md`), and the lookup the amend route runs.

**Why the write is AWAITED, alone among this route's side effects.** Everything else `/error-report` does after storing
the bundle rides `waitUntil` so the 200 ships fast. This one cannot: the same response hands the client its amend
credential, and a credential that opens nothing until some background work finishes is a race the client would lose. It
is one small KV put. ❌ Don't move it into `postUploadWork`. A put that throws costs the amend flow, not the report: the
bundle is already in R2, so the route logs it and answers 200 with `amendKey: null`, which clients read as "this one
can't be amended".

**Why a stored random credential rather than the id itself:** `ERR-XXXXX` is 31^5 ≈ 28.6 million values, and it is
printed in the send dialog, quoted back in emails, and pasted into chats. It identifies a report; it can't authorize
writing to one. The credential is 32 bytes from `crypto.getRandomValues`, base64url, returned exactly once in the upload
response. Only its SHA-256 is stored, so a KV dump is a pile of hashes rather than a pile of live credentials, and the
comparison goes through `constantTimeEqual` (`../licensing/paddle.ts`), the same one `verifyAdminAuth` uses.

**The 404 on an unknown id is a deliberate, bounded leak:** it tells a caller whether an id exists. Ids are shown to
users and are not secret, the credential check is what actually gates writing, and `ERROR_REPORT_AMEND_LIMITER` bounds
how fast the id space could be walked. Answering 401 for an unknown id instead would leave a real reporter with an
expired report unable to tell "gone" from "wrong key".

### Amendments (`error-report-amend.ts`)

`POST /error-report/:id/amend` is how a second thought lands on a report that is already stored: a note someone forgot,
or a reply-to address they decided to give after all. Without it the client's only move is a second upload, which mints
a second `ERR-XXXXX` and splits one incident across two unrelated bundles.

**Where amendments live:** a sidecar R2 object beside the bundle, the same key with `.zip` swapped for `.amend.json`
(`amendSidecarKey`), holding `{ id, amendments: [{ note, email, amendedAt }, ...] }`. Beside the bundle rather than in
KV or D1 because that is what keeps the two in step for free: the sidecar is evicted with its bundle, expires on the
same 90-day lifecycle, and lands in the same place a triager is already looking. `amendedAt` is the server clock.

The sidecar shares the `error-reports/prod/` prefix, so anything listing that prefix has to skip it: `tryEvict` pairs it
with its bundle (§ Eviction) and `/admin/error-reports` filters it out (`../admin/DETAILS.md`), because counting it
would double an amended report in every aggregation the dashboard runs.

**Read-modify-write, so a second amendment appends.** R2 has no append, so two amendments racing on one report can lose
the earlier one. Accepted: a person adding two notes to the same report in the same second isn't worth a lock, and the
email carries each note's text regardless.

**Rate limiting and budget:** its own `ERROR_REPORT_AMEND_LIMITER` (10/min/IP) rather than a share of the upload
limiter, which is tighter for a reason that doesn't apply here (a note is not a 10 MB bundle) and would otherwise turn
away a reporter whose upload just spent the allowance. The daily BYTE budget is deliberately not charged: a note is
bytes-tiny, and charging it would let notes push the bucket toward an intake pause.

**The email** reuses the report notification path and shares its daily allowance (`claimErrorReportEmailSlot`), charged
against TODAY rather than the report's upload date. A report can be amended months after it landed, and that day's
counter key expired long ago, so charging it would reset the cap to zero on every amendment to an old report. Sharing
the allowance rather than taking a second one is deliberate. It is the same inbox and the same conversation, and an
amendment needs a credential minted by an upload, so it is never the cheaper way to flood. It rides `waitUntil` after
the 200, like every other side effect here: the sidecar is already stored, so a mail problem is ours.

### Notification email (hand-written reports only)

`kind: 'user'` reports (the ones written in the "Send error report" dialog) are mailed the moment they land, one report
per email, alongside the Discord embed. `kind: 'auto'` never is.

**Why at intake rather than on the cron:** a bundle lives in R2 with no D1 row, so there is no `notified_at` column a
digest could drive, and a table for roughly four rows per two months earns nothing. Hand-written reports are rare and
each already carries a presigned link, so `postUploadWork` sends directly.

**Why auto-sends stay Discord-only:** one misbehaving install produced 50+ auto bundles in three days. Discord absorbs
that volume; an inbox does not, and the hand-written report that needs an answer would be buried in it.

**The cap** (`DAILY_ERROR_REPORT_EMAIL_CAP`, 10/UTC day, its own `error_email_count:{date}` KV key): `kind` is
client-supplied, so a build that mislabels its auto-sends can aim the inbox at itself. Ten is ~150x the observed
hand-written rate, so a genuinely bad day still arrives in full. Past it, one email says the rest of the day is
suppressed, then silence until tomorrow. That notice goes to the same inbox rather than Discord: the inbox is the
channel going quiet, and Discord is still getting every report anyway, so a notice there would explain a gap the reader
can't see.

**Failure isolation:** the send sits in its own try/catch inside `postUploadWork`, like every other side effect there.
The bundle is stored and the client has its 200 before any of this runs, so a rejected send is logged and dropped, never
surfaced to the reporter. Missing recipient or missing `RESEND_API_KEY` is a silent no-op.

**Reply-To:** when the reporter ticked "Attach my email", `meta.email` becomes the message's `Reply-To`, so answering
them is a plain reply. One report per email means there is always exactly one right address, unlike the feedback digest,
which sets the header only when a batch carries exactly one. The address is also rendered as a `mailto:` in the card
footer (`replyToLine`, shared with the feedback card).

**What it says:** the short id, the note with `white-space: pre-wrap` so the writer's line breaks survive, app version,
OS version, arch, bundle size, a `prod`/`dev` chip, the reply-to line, and the download link with its 7-day expiry
stated next to it (`linkTtlDays` is derived from `PRESIGN_TTL_SECONDS`, so the copy can't drift). Debug builds get a
`[DEV]` subject mark; release builds get none, because a tag on every ordinary report is noise in an inbox list.
Rendering lives in `../email/error-report.ts`.

### Eviction (8/6 GB watermarks + 60-day age floor + lifecycle)

Three layers keep the bucket bounded:

1. **On-upload eviction**: every `POST /error-report` schedules `tryEvict` in `waitUntil(...)`. If `total_bytes` (KV) >
   8 GB and `eviction_in_progress` (KV, 60-s TTL lock) isn't set, lists R2 objects under `error-reports/`, keeps only
   those older than `EVICTION_MIN_AGE_DAYS`, sorts oldest-first by the embedded `yyyy-mm-dd` segment then by `uploaded`,
   deletes until ≤ 6 GB, then resets the counter to the recomputed ground truth.
2. **Daily cron sweep**: corrects KV drift by recomputing from R2, lifts an intake pause once the bucket is back under
   the LOW watermark, and re-runs `tryEvict`.
3. **R2 lifecycle rule**: 90-day expiration applied at provisioning time via `../../scripts/setup-cf-infra.sh`.

**Amendment sidecars are never candidates in their own right.** `tryEvict` filters `.amend.json` objects out of the
candidate list and deletes each one with its bundle instead, counting both toward what that bundle frees. A sidecar is
always younger than the report it amends, so judging it separately would either orphan it (the bundle gone, the note
left counting against the bucket until the lifecycle got to it) or hold its bundle behind the age floor. Their bytes
still count in `total_bytes`, and `extractDateSegment` reads a sidecar key exactly like a bundle key, since it scans
segments for the date rather than parsing the filename. A sidecar written against a bundle that eviction already removed
(possible for up to 30 days, between the 60-day floor and the 90-day index TTL) is left to the lifecycle.

The KV counter is approximate (read-then-write, no atomic increment). Both the daily sweep and the post-eviction
recompute correct it. R2 deletes are idempotent, so concurrent evictors deleting the same oldest object cause no harm.

**Why the age floor exists (`EVICTION_MIN_AGE_DAYS`, 60 days):** `/error-report` is unauthenticated, so without a floor
anyone able to push the bucket past 8 GB turns eviction into a delete primitive aimed at the oldest (most likely
genuine) reports. Eviction's real job is only to pull the 90-day lifecycle forward under space pressure, so what it
deletes should already be near its natural end.

Eviction is therefore **all-or-nothing**: when the eligible bundles can't free enough on their own, `tryEvict` deletes
NOTHING, sets `intake_paused`, and returns `{ outcome: 'paused' }` so the caller alerts Discord. Half-evicting would
destroy real reports AND leave the bucket over its watermark. A flood of fresh junk finds nothing eligible and costs
zero deletions; reaching eligibility would take 60 days of sustained flooding, alerting daily along the way. A pause
reads as one of two things: a flood filled the bucket with fresh bundles, or real traffic outgrew the watermarks (raise
them, or shorten the lifecycle). The daily sweep clears the flag once the bucket is back under 6 GB; resuming at the
HIGH watermark would reopen intake straight into the level that paused it.

### Intake admission (`error-report-intake.ts`)

The global ceiling the per-colo rate limiter can't give. Both gates run before the body is read, so a rejected upload
costs no parsing and no storage.

- **Daily byte budget** (`DAILY_INTAKE_BUDGET_BYTES`, 2 GB/UTC day): past it, `/error-report` returns 503 +
  `Retry-After` for the rest of the day and pings Discord ONCE (`budget_alert:{date}` claim). Legitimate traffic is
  orders of magnitude below this, so the ping is as much the point as the rejection. It also means filling the 8 GB
  watermark takes days of flooding, alerting each day.
- **Intake pause** (`intake_paused`): 503 while set. Written by eviction, cleared by the daily sweep, and settable by
  hand for an incident (`wrangler kv key put --binding ERROR_REPORT_META intake_paused 1`).
- **Notification cap** (`DAILY_NOTIFICATION_CAP`, 50/day): per-upload Discord embeds stop after 50, with one notice
  saying so, then silence until tomorrow. A webhook takes 30 messages/min, and a channel that goes quiet without
  explanation reads as "no reports". Bundles remain in R2 and in `/admin/error-reports` regardless. Eviction and budget
  alerts are NOT capped.
- **Email cap** (`DAILY_ERROR_REPORT_EMAIL_CAP`, 10/day): the same three-way decision on its own KV key, so neither
  channel can silence the other. Both go through `claimDailySlot`. Sizing and routing: § Notification email above.

Every counter here is a racy read-then-write (KV has no atomic increment), so a concurrent burst can overshoot by
roughly the in-flight amount. Deliberate: these are coarse circuit breakers, and the 10 MB bundle cap keeps a single
overshoot small.

## In-app feedback

`POST /feedback` is the open-beta "Send feedback" channel. JSON body: required `feedback` text (trimmed, 1–100 000
Unicode code points; the cap matches the desktop dialog and the Rust validator) plus `appVersion` / `osVersion`,
optional reply-to `email` (loose shape check) and `buildMode`. Body capped at 512 KB. The D1 `feedback` table is the
durable sink, so unlike the other telemetry writes this one is AWAITED: a D1 failure returns a soft 502 the desktop app
surfaces as a gentle retry. The Discord ping (truncated preview, `[DEV]`/`[PROD]` title prefix from `buildMode`) rides
`waitUntil` after the 204. No install id of any kind is read or stored, so feedback can't be joined to the analytics
stream. Rate-limited at 5/min/IP via `FEEDBACK_LIMITER` (the IP is never stored).

Three sinks read the row: the `/admin/feedback` endpoint, the Discord `#feedback` channel, and the 3-hourly feedback
digest email, which is the one that reaches David unprompted. The digest tracks what it has sent in
`feedback.notified_at` (NULL means unsent); the intake path never writes that column. How the digest renders and when it
stamps: `../../DETAILS.md` § Cron handler.

Frontend counterpart: `apps/desktop/src/lib/feedback/CLAUDE.md`.

## Gotchas

- **The `/download/:version/:arch` redirect maps `x86_64` → `x64` in the filename.** `tauri-action` names the Intel DMG
  `Cmdr_<ver>_x64.dmg`, but the rest of the codebase (URL path, D1 telemetry, website data attrs, Rust target triple,
  `uname -m`) consistently uses `x86_64`. Mapping at the boundary keeps everything else canonical; the same convention
  is in `.github/workflows/release.yml` when reading DMG sizes for `latest.json`.
- **Validators for optional fields must tolerate both `null` and `undefined`.** serde `Option::None` serializes as JSON
  `null`, not as an absent key, and `#[serde(skip_serializing_if)]` is rejected by `specta`'s unified mode (the struct
  is part of a Tauri command surface). An old crash file read by a new client surfaces missing fields as `None`, the
  client posts `"buildMode": null`, and a `!== undefined`-only check rejects exactly the upgrade-window reports worth
  keeping.
