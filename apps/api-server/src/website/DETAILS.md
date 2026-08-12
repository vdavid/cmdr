# Website endpoints details

Pull-tier docs for `src/website/`. Must-know invariants live in `CLAUDE.md`; secrets, KV bindings, and rate-limit
bindings live in `../../DETAILS.md`.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`beta-signup.ts`**: `POST /beta-signup` — email-only Listmonk double-opt-in subscribe, NO install id.
- **`likes.ts`**: `/likes/:slug` (GET, POST, DELETE, OPTIONS) — blog-post hearts keyed by a per-post IP pseudonym.
- **`link-codes.ts`**: `GET /r-codes.json` (public, edge-cached) plus `/admin/r-codes` CRUD. The pure `sanitizeUtmValue`
  and `isValidCode` are unit-tested.
- Tests: `beta-signup.test.ts` (the Listmonk call, the no-install-id invariant, soft failure, rate limit),
  `likes.test.ts` (the slug gate, the rate limit, the salt requirement, and the pseudonym's per-salt/per-slug/per-IP
  separation), `link-codes.test.ts` (the public map, CORS, cache, admin CRUD auth, and the validators).

## Beta signup (decoupled, contact-only)

`POST /beta-signup` is the contact channel for early testers. It reads ONLY the `email` from the body and subscribes it
to the double-opt-in Listmonk list `LISTMONK_BETA_LIST_ID` (`POST https://mail.getcmdr.com/api/subscribers`,
`Authorization: token <LISTMONK_API_USER>:<LISTMONK_API_TOKEN>`, subscriber `status: "enabled"` — the subscriber-status
enum only accepts enabled/disabled/blocklisted, while `"unconfirmed"` is the per-LIST subscription status — and
deliberately NO `preconfirm_subscriptions`, so Listmonk sends its own confirmation email). The privacy invariant is the
whole point: the request carries NO install id of any kind, so the email and the analytics ids never co-occur on our
servers (guarded by `beta-signup.test.ts`, including the outbound Discord payload).

On a Listmonk network/5xx failure it returns a soft 502 the desktop app surfaces as a gentle "try again" (NOT
fire-and-forget: we want the user to know it didn't land). Missing Listmonk config returns 500. The list id is a
wrangler `[var]`, not a secret; see `docs/tooling/listmonk.md`.

**409 add-to-list recovery:** a 409 ("subscriber already exists" — for example they're on the newsletter list) used to
map straight to 204, which left that person OFF the beta list. Now a 409 triggers a lookup
(`GET /api/subscribers?query=subscribers.email='<addr>'`); if they're not yet on the beta list, the route adds it
(`PUT /api/subscribers/lists`, `action: "add"`, `status: "unconfirmed"`) and then explicitly sends the opt-in mail
(`POST /api/subscribers/{id}/optin`). The optin call is REQUIRED: the list-add endpoint does NOT send the confirmation
email on its own (verified against Listmonk's `ManageSubscriberLists` handler), so without it consent would be silently
implied. A subscriber already on the beta list is a quiet re-signup: no list change, no mail, no ping. Every outcome
returns the identical empty 204, so the response never reveals whether the address existed.

**Discord ping:** a successful signup pings Discord (`DISCORD_BETA_SIGNUP_WEBHOOK_URL`, falling back to
`DISCORD_WEBHOOK_URL` so it works before the `#beta-signups` channel exists) in `waitUntil` after the 204 ships,
drop-on-failure. It fires ONLY when a beta subscription was newly established (a fresh 2xx, or the 409 add-to-list
path), NEVER on a Listmonk failure and NEVER on a plain already-on-list 409. The embed carries the email (full, same
precedent as the feedback reply-to) and the signup time, and states the honest consent status ("unconfirmed — Listmonk
sent the confirmation email" for both paths). It carries no install id, by construction.

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
