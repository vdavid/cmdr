# Website endpoints details

Pull-tier docs for `src/website/`. Must-know invariants live in `CLAUDE.md`; secrets, KV bindings, and rate-limit
bindings live in `../../DETAILS.md`.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`listmonk-signup.ts`**: `handleListSignup`, the whole signup flow both routes below share.
- **`beta-signup.ts`**: `POST /beta-signup` — the app's email-only beta-list subscribe, NO install id.
- **`newsletter-signup.ts`**: `POST /newsletter-signup` (plus its `OPTIONS` preflight) — getcmdr.com's newsletter form,
  CORS for getcmdr.com origins only.
- **`likes.ts`**: `/likes/:slug` (GET, POST, DELETE, OPTIONS) — blog-post hearts keyed by a per-post IP pseudonym.
- **`link-codes.ts`**: `GET /r-codes.json` (public, edge-cached) plus `/admin/r-codes` CRUD. The pure `sanitizeUtmValue`
  and `isValidCode` are unit-tested.
- **`csp-report.ts`**: `POST /csp-report` (plus its `OPTIONS` preflight) — getcmdr.com's CSP violation reports, our own
  breakage alerted to Discord.
- Tests: `csp-report.test.ts` (both report formats, the noise filter, the daily dedupe, query stripping, the size cap,
  the rate limit, CORS), `listmonk-signup.test.ts` (both signup routes: the Listmonk call, the failed-send 502, no
  enumeration, the no-install-id invariant, rate limit, Discord, the newsletter CORS), `likes.test.ts` (the slug gate,
  the rate limit, the salt requirement, and the pseudonym's per-salt/per-slug/per-IP separation), `link-codes.test.ts`
  (the public map, CORS, cache, admin CRUD auth, and the validators).

## Signups (beta and newsletter)

Two routes, one flow (`listmonk-signup.ts::handleListSignup`): `POST /beta-signup` is the desktop app's contact channel
for early testers, and `POST /newsletter-signup` is getcmdr.com's newsletter form. Each reads ONLY the `email` from a 1
KB-capped body and subscribes it to its double-opt-in Listmonk list by UUID (`LISTMONK_BETA_LIST_UUID`,
`LISTMONK_NEWSLETTER_LIST_UUID`, wrangler `[vars]`), gated by the shared `SIGNUP_LIMITER` (5/min/IP).

**Decision: Listmonk's public endpoint, not the admin API.** The call is
`POST <LISTMONK_API_URL>/api/public/subscription` with `{email, list_uuids: [uuid]}` and no credentials. Why (verified
by reading listmonk v6.2.0's `CreateSubscriber`, `processSubForm`, and `InsertSubscriber` handlers, 2026-10-05):

- The admin `POST /api/subscribers` inserts with `assertOptin=false`: when the opt-in mail fails to send, it logs and
  still answers 200. From 2026-06 to 2026-10, Listmonk's Resend SMTP key was invalid, so every beta signup got a 204 and
  a Discord ping saying the confirmation went out, while nothing was sent. The public endpoint asserts the send and
  answers 500, which we turn into a soft 502.
- On an existing address, the public endpoint adds the list itself and mails a confirmation for every still-unconfirmed
  double-opt-in list (`UpdateSubscriberWithLists`). That covers what a hand-rolled 409 lookup + list-add + `optin` call
  used to do, and also re-sends to someone stuck as unconfirmed, whom the old "already on the list" branch silenced.
- It has no `preconfirm` field, so a prank signup can't subscribe someone else's address, and it needs no API token.
- It refuses `private` lists, so both lists must stay `public` in Listmonk (they are double opt-in either way).

**Outcomes.** `{data: {has_optin: true}}` means Listmonk mailed a confirmation: 204 plus a Discord ping.
`has_optin: false` means there was nothing to confirm (already confirmed): the same empty 204, no ping. A Listmonk 400
(its email check is stricter than ours) is a 400. A 5xx or a network failure is a soft 502, which the app and the
website form both show as a gentle "try again". A missing URL or list UUID is a 500. Every success looks identical, so
the response never reveals whether an address existed.

**The beta privacy invariant:** `/beta-signup` carries NO install id of any kind, so the email and the analytics ids
never co-occur on our servers (guarded by `listmonk-signup.test.ts`, including the outbound Discord payload).

**Discord ping:** sent only when Listmonk mailed a confirmation, to `DISCORD_BETA_SIGNUP_WEBHOOK_URL` (both lists; the
name predates the newsletter route), falling back to `DISCORD_WEBHOOK_URL`, in `waitUntil` after the 204 ships,
drop-on-failure. The embed names the list, the signup time, and a Listmonk admin link. It carries no email and no
install id, by construction: `ListSignupNotification` has no field for either (`apps/api-server/DETAILS.md` § Discord
webhooks).

## Blog likes

`/likes/:slug` stores one KV key per post (`likes:<slug>` in the `BLOG_LIKES` namespace) holding the count and the
caller pseudonyms. `GET` is public and returns the count plus whether this caller already liked it; `POST` (like) and
`DELETE` (unlike) are idempotent per pseudonym and gated by `LIKES_LIMITER` at 20 req/min/IP; `OPTIONS` is a 204 CORS
preflight for getcmdr.com origins only.

**Decision: the pseudonym is salted with the post SLUG, not the UTC day.** The stored value has to be stable per reader
for years (that's what "you already liked this" means) yet never recoverable to an IP. The daily salt telemetry uses
would forget every like overnight, so likes pass the slug instead: still public, still no secrecy of its own, and it
buys the same unlinkability in the dimension that matters here (one reader gets an unrelated pseudonym on every post, so
a KV dump can't be pivoted into a per-person reading history). The pepper does the one-way work in both. Writing a
second hashing scheme would have meant two places to get the pepper rule right; there's one.

**Decision: the pseudonym is truncated to 16 hex chars**, unlike the full digest telemetry stores. It's stored once per
liker per post, so the full 64 chars would quadruple every KV value. Collisions at these counts would cost at most one
reader a heart that was already filled.

**Decision: validate the slug against the blog's own charset before touching KV.** `POST` creates the key it writes and
takes no auth, so an unvalidated slug is an unbounded KV-growth primitive (and a way to run up the bill). The route
can't check the slug against the real post list (the Worker doesn't know it), so the charset plus an 80-char cap plus
the rate limiter is the bound.

**Pepper caveat:** KV has no retention sweep, so `likes:<slug>` values written while `IP_HASH_PEPPER` was missing stay
weakly hashed until the keys are deleted. Recovery: `wrangler kv key list --binding BLOG_LIKES`, delete, let the counts
rebuild. (Telemetry rows self-heal through the retention sweep instead; `../../DETAILS.md` § Deployment.)

## CSP reports

getcmdr.com's CSP (`apps/website/nginx-security-headers.conf`) names `/csp-report` as both its `report-uri` (Firefox,
Safari: one `{"csp-report": {…}}` per violation) and its `report-to` endpoint (Chromium: a Reporting API batch,
`[{type: "csp-violation", body: {…}}]`). `parseCspReports` normalizes both and drops anything else.

**Decision: alert only on violations that look like our own breakage.** Every visitor's browser reports, and most raw
volume is extensions injecting scripts and styles. `isActionableViolation` keeps a report only when the page is
getcmdr.com (`https`), the blocked value is an `http(s)` URL (not `inline`, `eval`, `data`, `blob`, or an extension
resource), and the source file, when given, is an `http(s)` URL (not extension code). `font-src` never alerts: every
font is self-hosted, so a blocked one is always an extension's or a browser's (Perplexity's, scite's, Google Fonts), and
some arrive attributed to our own PostHog recorder, which re-applies injected styles. Paddle Retain's `profitwell.js`
never alerts either (only that script URL, only under `script-src*`): Paddle.js loads it on every live checkout page
with no setting to stop it, we pass no `pwCustomer` so Retain has no work there, and allowing it would add a tracker the
privacy policy doesn't cover. The Discord alert then fires once per `(directive, blocked origin)` a day, deduped in
`CSP_ALERTS`. Every actionable report also goes to the Workers log (`console.warn`), so the count is there even when
Discord stays quiet.

**Privacy:** nothing about the visitor is stored. The KV key holds a directive and an origin, the IP only feeds the rate
limiter, and page and blocked URLs lose their query strings before they reach the log or Discord.

The route always answers 204 (a malformed body too: a browser never sends one and there's nothing to tell the sender),
except 413 over the 32 KB cap and 429 from `CSP_REPORT_LIMITER`. `OPTIONS` answers the Reporting API's CORS preflight
for getcmdr.com origins only; the legacy `report-uri` POST needs none.

## Link codes (`?r=` tracking links)

Short, inconspicuous `?r=<code>` links (for example `getcmdr.com/?r=rmc`) expand to UTM params client-side on
getcmdr.com and David's blog. The code → meaning map lets David invent a new code without a code change or a deploy.

- **KV model:** the WHOLE map lives under ONE key (`codes`) in the `LINK_CODES` namespace, as JSON
  `{ "<code>": { "utm_source": "...", "utm_medium": "...", "note": "..." }, ... }`. The map is tiny (a handful of
  channels), so one blob keeps the public endpoint a single KV get and makes a write a trivial read-modify-write of one
  value. Key-per-code would buy nothing here.
- **Public endpoint `GET /r-codes.json`:** returns the map with the admin-only `note` stripped (source + medium only),
  `Access-Control-Allow-Origin: *` (public non-sensitive config, fetched cross-origin from both getcmdr.com and the blog
  at veszelovszki.com), and `Cache-Control: public, max-age=300`. The 5-minute edge cache keeps blog page loads off KV;
  a new code is live within the TTL. CORS preflight is `OPTIONS` → 204.
- **Admin CRUD (`/admin/r-codes*`, Bearer `ADMIN_API_TOKEN`):** `GET` lists the full map (with notes);
  `PUT /admin/r-codes/:code` upserts; `DELETE /admin/r-codes/:code` removes. The path `:code` must match `[a-z0-9._-]`,
  1..64 chars (`isValidCode`), else 400. `utm_source` is required and `utm_medium` / `note` are optional; UTM values run
  through `sanitizeUtmValue` (lowercase, drop outside `[a-z0-9._-]`, cap 120) — a source that sanitizes to empty is
  rejected 400. `note` is capped at 500 chars and never leaves the admin endpoint.
- **Charset is the contract:** `sanitizeUtmValue` mirrors the blogs' client-side sanitizer and the `/download` `ref`
  rule (`../telemetry/DETAILS.md` § Download tracking), so a stored value and a client pass-through value normalize
  identically. The end-to-end attribution story: `docs/architecture.md` § Acquisition analytics.
