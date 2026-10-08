# Licensing details

Pull-tier docs for `src/licensing/`. Must-know invariants live in `CLAUDE.md`; app-wide configuration, secrets, and
deploy runbooks live in `../../DETAILS.md`.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Files

- **`licensing.ts`**: routes `/webhook/paddle`, `/activate`, `/validate`, and the mount for `manual-licenses.ts`.
- **`manual-licenses.ts`**: `/admin/generate` and `/admin/revoke`, the licenses we hand out rather than sell.
- **`admin-licenses.ts`**: `GET /admin/licenses` and `PUT /admin/licenses/:transactionId/note`, the dashboard's view of
  every license we've issued and the one write on it, plus the pure `classifyLedgerEntry`. § The licenses listing.
- **`refunds.ts`**: the `adjustment.*` half of the webhook: records every refund, chargeback, and credit
  (`license_adjustments`), decides which revoke (`classifyAdjustment`, pure), and lists them for the dashboard. §
  Refunds.
- **`license-backup.ts`**: the daily R2 snapshot of the ledger and the key store. § License backups.
- **`license.ts`**: short-code and license-key generation, the `LicenseType` enum, `isPaddleTransactionId` /
  `generateManualTransactionId` (the id namespaces `/validate` dispatches on), and `generateShortId(prefix, len)` (also
  used for the `ERR-XXXXX` error-report ids).
- **`license-issuance.ts`**: the D1 ledger (`license_issuance`) behind both kinds of license: claim, take-over, code
  storage, and delivery marking for a Paddle fulfillment; row writing, lookup, and revocation for a manual one; note
  editing and the two reads (`listLedger`, capped, for the dashboard; `readWholeLedger`, uncapped, for the backup);
  refund revocation (`revokePaddleLicense`, `isPaddleLicenseRevoked`); plus the pure `classifyIssuance` and
  `classifyManualLicense`.
- **`paddle.ts`**: HMAC-SHA256 webhook verification and `constantTimeEqual` (the timing-safe compare every bearer-token
  check in the Worker uses).
- **`paddle-api.ts`**: Paddle REST client (transaction / subscription / customer fetch, `getLicenseTypeFromPriceId`).
- **`device-tracking.ts`**: device-set helpers — prune stale devices, alert threshold.
- Tests: `license.test.ts`, `paddle.test.ts`, `license-issuance.test.ts` (the two pure classifiers), `refunds.test.ts`
  (`classifyAdjustment`), `device-tracking.test.ts`, `webhook-paddle.test.ts` (first delivery, duplicate, retry after a
  failed email, concurrent delivery, Resend rejection), `admin-licenses.test.ts` (the states, the orphan and
  missing-code reconciliation), and two real-runtime suites that run the built Worker in workerd (`../../DETAILS.md` §
  Test runtimes): `production-runtime.test.ts` (minting, manual validation, revocation) and `webhook-runtime.test.ts`
  (the purchase and refund paths end to end, with Paddle and Resend stubbed at the socket).

## Data flow

```
Paddle webhook → HMAC verify (tries both live + sandbox secrets)
  → transaction.completed with a `subscription_*` origin (renewal etc.) → 200 ignored, nothing issued
  → claim the transaction (D1 license_issuance, conditional INSERT; see Fulfillment below)
  → Paddle API: fetch customer details
  → per seat: generateLicenseKey() → generateShortCode() → KV.put(code, {fullKey, orgName})
  → store the codes on the row (short_codes, issued_at)
  → sendLicenseEmail() via Resend
  → mark the row delivered (emailed_at)

Paddle adjustment webhook (refund, chargeback, credit) → same HMAC verify
  → upsert the adjustment (D1 license_adjustments)
  → approved full refund or approved chargeback: set revoked_at on the license_issuance row, delete its codes from KV

App activation: POST /activate → KV.get(shortCode) → return fullKey

Validation: POST /validate → dispatch on the transaction id's namespace
  `txn_...` → ledger: revoked_at set (refunded) → HTTP 200 + invalid, Paddle never asked
    → otherwise Paddle API transactions + subscriptions
    → HTTP 200 + ValidationResponse on success or invalid transaction (Paddle 404)
    → HTTP 502 + { error: "upstream_error" } if Paddle API unreachable or returns server error
    → if deviceId present: track device in KV (devices:{seatTransactionId}), log to Analytics Engine
    → if device count >= 6 and not recently alerted: send alert email to legal@getcmdr.com
  anything else → D1 license_issuance row where source = 'manual' (see Manual licenses below)
    → HTTP 200 + ValidationResponse, or HTTP 502 if the ledger read throws
  then, on a 200 with a valid nonce: + signedAnswer (see Key formats)
```

## Key formats

- **Short code:** `CMDR-XXXX-XXXX-XXXX` using 31 unambiguous chars (excludes 0/O/1/I/L). Rejection sampling avoids
  modulo bias (max unbiased byte = `256 - (256 % 31)`).
- **License key:** `base64(JSON payload).base64(Ed25519 signature)`. Payload: email, transactionId, issuedAt, type,
  organizationName, shortCode, and `expiresAt` on a hand-issued dated license only (signed in so the app enforces the
  date offline; a renewing Paddle subscription has no fixed date to sign). The license email carries this full key
  beside the short code, so a buyer can activate without `/activate`.
- **Signed validation answer:** when the app sends a `nonce` (32 lowercase hex, `isValidNonce`), a 200 from `/validate`
  also carries `signedAnswer: { payload, signature }`. `payload` is base64 JSON (`transactionId`, `nonce`, `status`,
  `type`, `organizationName`, `expiresAt`, `signedAt`); `signature` is the license key's Ed25519 signature over
  `validationAnswerSignaturePrefix` + the payload bytes, so it never verifies as a license key. The app trusts nothing
  else, so this is what makes a revocation authentic. Why, and what the app does with it:
  `apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signed validation answers. ❌ Never sign without a valid nonce:
  that answer could be replayed. A 502 is never signed.
- **License types:** `commercial_subscription` | `commercial_perpetual`.
- **Short ids:** `generateShortId(prefix, len)` produces `ERR-A2345`-shaped ids from the same unambiguous alphabet
  (`23456789ABCDEFGHJKMNPQRSTUVWXYZ`), rejection-sampled. The error-report route consumes it (`../telemetry/`).

## Fulfillment (exactly-once issuance, at-least-once delivery)

A purchase must yield ONE set of license codes, but the email carrying them is safe to repeat. `license-issuance.ts`
keeps those apart, on the D1 table `license_issuance` (migration `0012`), one row per Paddle transaction:

1. **Claim**: `INSERT ... ON CONFLICT(transaction_id) DO NOTHING RETURNING transaction_id`, before any side effect. Two
   concurrent deliveries race on one primary key and SQLite hands the row to exactly one of them. This is the whole
   atomicity guarantee, and the reason the record lives in D1 rather than KV: KV has no conditional write, and its reads
   are eventually consistent (a redelivery within the propagation window would read a stale "not processed yet").
2. **Mint**: one signed license per seat, each stored in KV under its short code, then `short_codes` + `issued_at` on
   the row. Storing before sending is what makes a later redelivery reuse the SAME codes.
3. **Deliver**: `sendLicenseEmail`, then `emailed_at`. Only now is the purchase fulfilled.

A delivery that loses the claim reads the row and classifies it (pure `classifyIssuance`, unit-tested):

- `revoked` (`revoked_at` set: refunded or charged back) → 200 `revoked`, nothing minted or mailed. Checked first, so a
  refund that lands while a stuck fulfillment is still being retried wins (§ Refunds).
- `delivered` (`emailed_at` set) → 200 `already_processed`, forever.
- `in_flight` (claimed under `issuanceStaleAfterMs`, 5 min) → **503**, so Paddle redelivers instead of us running a
  second issuance beside the first. Live retries are 60 attempts over 3 days (20 in the first hour), so a transient 503
  costs minutes, and the buyer's licenses are never at stake.
- `resend` (stale claim, codes stored) → take over and re-send those codes. A duplicate email is the worst case.
- `remint` (stale claim, no codes: the delivery died before minting) → take over and mint. Any codes a dead attempt
  wrote to KV before failing are orphaned, which is harmless: nobody has seen them.

Take-over is `UPDATE ... WHERE claimed_at = <the value we read>`, so when two deliveries both find a stale claim, only
one wins and the other gets the 503.

**Rows never expire.** "This purchase was fulfilled" has no useful end date, and an expiring marker is exactly how a
late redelivery or a replayed webhook mints a second set of usable perpetual licenses. The table also doubles as the
support/audit trail (who got which codes, when).

**Only a purchase fulfills, never a subscription's follow-up.** Paddle completes a NEW transaction (new `txn_` id) for
every renewal, one-off charge, plan or seat change, and payment-method update, with `origin` set to
`subscription_recurring` / `subscription_charge` / `subscription_update` / `subscription_payment_method_change`. The
buyer's key names the subscription's first transaction and keeps validating through the subscription's status, so those
are acknowledged and ignored before the claim (`isSubscriptionFollowUp`, `licensing.ts`); before this, every renewal
mailed a fresh set of keys. A missing or unknown origin still fulfills: a paying buyer without a key is the worse miss.
Gotcha: a seat INCREASE on a subscription is a `subscription_update` too, so extra seats are not issued automatically;
mint them by hand until that's built.

**Decision, why not the Paddle `event_id` as the key:** one purchase must yield one set of licenses however many events
carry it, so the transaction id is the unit of fulfillment. `event_id` is stored on the row for debugging only.

**Gotcha: Resend reports failures in its response, it doesn't throw.** All four senders go through `sendViaResend`
(`../email/send.ts`), which turns an `error` into a thrown one. The app-wide rule is in `../../CLAUDE.md`; the money
consequence is here: an unchecked `await` marks the purchase delivered and stops Paddle retrying, so the buyer pays and
gets nothing.

**Decision: a webhook's `ts` must sit within `PADDLE_TIMESTAMP_TOLERANCE_SECONDS` (300 s) of now, either way.** The
timestamp is inside the HMAC, so a captured webhook can't be freshened. Paddle defines `ts` as when the webhook was
SENT, and its SDKs default to a five-second window on every delivery of a three-day retry schedule, which only works if
each retry is re-signed (https://developer.paddle.com/webhooks/about/signature-verification, checked 2026-10-05). Five
minutes leaves room for clock skew and a slow hop. The fulfillment row still backs it: a replay inside the window finds
`emailed_at` and does nothing. If a live delivery is ever refused for its age, Paddle retries it, and the dashboard's
notification log shows it.

**`/activate` is rate-limited per IP** (`ACTIVATE_LIMITER`, 10/min): a short code is all it takes to fetch a full key.
**`/validate` is too, loosely** (`VALIDATE_LIMITER`, 60/min): each request costs a Paddle call, but a company's Macs
share one NAT address. A 429 is safe: the app reads any non-502 failure like a network error, keeps its cached status,
and retries after its cooldown (`apps/desktop/src-tauri/src/licensing/validation_client.rs`). Both limits and their
bindings: `../../DETAILS.md` § Configuration; pinned by `rate-limits.test.ts`.

## Manual licenses

Licenses we hand out rather than sell: an evaluation for a prospect on a work machine, free seats for partner companies,
thank-yous for testimonials and bug reports, customer-service recovery. They're issued constantly, so the process is two
scripts rather than a runbook of `curl` calls.

**The ledger row IS the license.** There's no Paddle transaction to resolve against, so `license_issuance` carries
everything `/validate` needs: `source = 'manual'`, `license_type`, `organization_name`, `expires_at` (NULL = perpetual),
`revoked_at`, and `note`. Migration `0017` added those columns; existing rows backfilled to `source = 'paddle'`.

**Dispatch is by id namespace, in `handleValidation`.** Paddle ids start with `txn_` in both environments, so anything
else is one of ours. A manual id is `manual-` plus 12 unambiguous characters (`generateManualTransactionId`), random
rather than time-based because the id travels inside the signed payload and is what `/validate` looks up: a guessable
one invites probing for other people's licenses. Random also keeps it clear of the `-\d+$` seat suffix the Paddle path
strips.

`classifyManualLicense` (pure, unit-tested) decides: `revoked_at` set → `invalid`, `expires_at` past → `expired`,
otherwise `active`. A missing row is `invalid`; a ledger read that THROWS is 502 `upstream_error`, the same answer a
Paddle outage gets, so a D1 blip can't drop a working license to Personal.

**Minting** (`/admin/generate`) writes the ledger row before the KV entry, then emails only if asked:

```bash
node apps/api-server/scripts/mint-license.js --email dana@example.com --org Acme \
  --note "Dana Lee, Acme, evaluation" --expires 90d --send
```

`--expires` takes a date (`2027-01-31`, meaning through the end of that day) or a span (`30d`, `6m`, `1y`); omit it for
a perpetual license. `--dry-run` prints the request. Without `--send` the code is only printed, which is what you want
when you're pasting it into a reply yourself.

- **A note is required**, in the script and in the route. A free license nobody can explain later is worse than no
  record, and this table is the only place that explanation can live.
- **The type follows the expiry**, exactly as it does for a purchase: dated → `commercial_subscription`, undated →
  `commercial_perpetual`. An explicit `type` is accepted only when it agrees, so "perpetual until March" is
  unrepresentable.
- **A rejected email answers 502 with the code in the body.** The license exists and works; only delivery failed, so the
  script prints the code to hand over rather than inviting a second mint.
- **The email says what's true of THIS license.** `sendLicenseEmail` takes `expiresAt` and `issuedManually`, which pick
  between three validity sentences (perpetual, dated, renewing) and drop the thanks-for-your-purchase opener for a
  license nobody bought. ❌ Never let the subscription wording reach a dated license: a prospect forwards this mail to
  their IT department, and "will auto-renew" on an evaluation that simply stops is the sentence that ends a deal. Pinned
  by `../email/license.test.ts`.
- **Every license email carries each seat's full key** ("Offline key") under its short code, and `/admin/generate`
  returns `fullKey` (the mint script prints it). A Paddle resend reads the keys back from KV (`readStoredLicenses`); a
  code missing there still goes out alone rather than holding the email back.

**Revoking** (`/admin/revoke`) sets `revoked_at` and deletes the short codes from KV:

```bash
node apps/api-server/scripts/revoke-license.js --code CMDR-XXXX-XXXX-XXXX
```

The code stops activating immediately; machines already running on it fall back to Personal at their next successful
check (within seven days when online). The full key from the email still verifies offline, so a Mac that never reaches
the server keeps the license: the app drops a license only on a signed `invalid`, never on silence. Reinstating means
minting a new license, there's no un-revoke. It takes `--transaction-id` too, and ❌ refuses a `txn_` id: refund or
cancel a real purchase in Paddle instead, so Paddle's books and our ledger never disagree. A full refund revokes the
license on its own (§ Refunds).

**Both scripts read `ADMIN_API_TOKEN`** from sops (`secret CMDR_ADMIN_API_TOKEN`) or the env var of the same name, never
from an argument, so the credential stays out of shell history.

**Gotcha: device tracking doesn't run on the manual path.** The fair-use alert resolves the customer through the Paddle
API, and a manual license has no Paddle customer. A shared hand-issued key is therefore invisible to the 6-device alert;
revocation is the lever.

## The licenses listing (`GET /admin/licenses`)

What the private dashboard's Licenses page reads, and the only place any question about license codes can be answered:
Paddle's dashboard knows about money, not about codes, activations, or anything we handed out by hand. One call returns
every row of the ledger (`listLedger`, newest claim first, capped at `ledgerListLimit` = 1000) with its columns mapped
to camelCase, plus a computed `state`.

`classifyLedgerEntry` (pure, unit-tested) decides the state, in this order:

- `revoked`: `revokedAt` set, by `/admin/revoke` or by a refund (§ Refunds). Its codes are gone from KV by design.
- `expired`: past `expiresAt` (or an unreadable one, the same call `classifyManualLicense` makes, so the page and the
  app never disagree in front of the same person). Only a hand-issued license carries an expiry.
- `unfinished`: claimed, never minted. A delivery that died, or one in flight this second.
- `undelivered`: minted but never emailed, and only for a `paddle` row: someone paid and is waiting. A manual license is
  often deliberately not emailed (the code goes into a reply by hand), so the same shape there is normal.
- `active`: everything else.

**`active` on a Paddle row means "we fulfilled this purchase", never "the subscription is still running."** Only Paddle
knows the latter, and asking it per row would cost one API call per license on every page load. The vocabulary is
therefore deliberately NOT `/validate`'s (`active` / `expired` / `invalid`); for a manual row the two agree, except that
`/validate` calls a revoked license `invalid`.

**The orphan check is the point of reading KV at all.** The endpoint scans the `LICENSE_CODES` namespace (paging through
`list`, keeping only keys that match the short-code format, since device sets and the activation counter share the
namespace) and reconciles it against the ledger both ways:

- `orphanCodes`: in KV, explained by no row. A license handed out before the ledger existed, or minted by a delivery
  that died before recording it. Someone may be holding one, and nothing in our records says whether it ever worked.
- `missingCodes`: on a row, absent from KV, so it can't be activated. Revoked rows are excluded, since revoking deletes
  their codes on purpose.

Both are read-only observations. ❌ Don't make this endpoint repair what it finds: an orphan needs a human to decide
whether to honor, ledger, or ignore it.

### Editing a note (`PUT /admin/licenses/:transactionId/note`)

The dashboard's edit dialog, and the only write on the listing side. It takes the WHOLE note rather than appending: the
dialog opens holding what's already there, so the person edits in place and saves the result.

Any row, either source. A purchase starts with no note at all, and this is the only way one gets there, which matters
because a fact about a sale ("first purchase ever", "asked about SFTP on 2026-09-16") has nowhere else to live beside
the license it's about. Paddle holds the money and knows nothing else.

- A blank or whitespace-only note clears a `paddle` row's note, so the dialog needs no separate clear control.
- The same blank on a `manual` row is refused (`manual_needs_note`). Minting refuses without a note for a reason, and an
  edit that could blank it would walk straight around that guardrail.
- The cap is `maxLicenseNoteLength` (`types.ts`), shared with minting so a note written at mint time can always be
  edited back to its own length.

**The dashboard's copy of the note belongs to the LICENSE, not the person.** Who someone is, what they said, and what
they're worth commercially live in David's vault; a second home for the same material gives two half-complete records
and no rule for which one to trust.

## License backups

`handleLicenseBackup` (cron, 00:00 UTC, first in the daily block) writes one R2 object per day under
`backups/licenses/YYYY-MM-DD.json`. `license-backup.ts` owns it.

**Why it exists is the asymmetry between the two stores.** D1 carries 30 days of Time Travel, so the ledger survives a
mistake on its own. KV carries no point-in-time recovery at all, so losing `LICENSE_CODES` loses every signed key while
the rows describing them stay. Reconstructing a key from a row is not reliable either: the signed payload carries the
mint-time `issuedAt`, and the row's `issued_at` is written a moment later, so a regenerated key would differ. The
snapshot therefore holds BOTH halves and is restorable on its own, with no signing key and nothing re-minted:

- `licenses`: every ledger row, oldest first, through `readWholeLedger` (uncapped, unlike `listLedger`: a listing that
  truncates costs a scroll, a backup that truncates loses a license).
- `keys`: every short code in KV with the record stored under it, **including codes no ledger row explains**. Those are
  the ones nothing else could restore.

A restore writes each `keys` entry back under its code and each `licenses` row back into `license_issuance`. There is no
restore script: doing it by hand from a known-good day is the point, and a script would be the more dangerous half.

- **The prefix is load-bearing.** `backups/` sits outside `error-reports/`, which the eviction sweep, both size
  watermarks, and the bucket's 90-day lifecycle rule are scoped to, so nothing here is swept, counted, or expired.
- **Append-only, deliberately.** Nothing prunes these; each day is a few kilobytes. Add a rule when there are enough
  objects for one to be worth writing.
- A second run on the same day overwrites that day's object, so a retried tick can't leave two versions of one day.
- A key that vanishes between the KV list and the read is left out rather than stored as null: the snapshot says what
  existed, and a null would read as a license with no key.

## Webhook verification

`verifyPaddleWebhookMulti` tries both `PADDLE_WEBHOOK_SECRET_LIVE` and `PADDLE_WEBHOOK_SECRET_SANDBOX` when verifying
incoming webhooks. This is a safety net; in practice the sandbox dashboard sends webhooks only to the sandbox
destination (ngrok for local dev), and the live dashboard only to `api.getcmdr.com`. If a sandbox webhook somehow
reaches the production endpoint (or vice versa), it still verifies rather than silently failing. Costs one extra HMAC
check on mismatch.

**Price ID → license type mapping:** `getLicenseTypeFromPriceId()` (`paddle-api.ts`) maps Paddle price IDs (from
`PRICE_ID_*` env vars) to license types. Unknown price IDs fall back to `commercial_subscription` for backwards
compatibility.

## Refunds

**Why a webhook:** a refund in Paddle is an adjustment that leaves the transaction itself `completed`, so
`getSubscriptionStatus` keeps answering `active` for a refunded one-time purchase. Only our own record can say
otherwise. `refunds.ts` handles `adjustment.created` and `adjustment.updated` on the same `/webhook/paddle` route,
behind the same HMAC check and 300 s replay window (§ Fulfillment).

**How it reaches the Mac:** the webhook sets `revoked_at` on the purchase's `license_issuance` row (and deletes its
codes from KV, so they stop activating). The `txn_` path of `/validate` reads that column BEFORE asking Paddle, strips
the seat suffix first so every seat of the purchase goes together, and answers a signed `invalid`. That's the one answer
the app drops a perpetual license on (`apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signed validation answers),
within its seven-day check. A ledger read that throws answers 502, like a Paddle outage. The ledger only ever takes a
Paddle license AWAY: whether one is active, expired, or which type it is still comes from Paddle.

**What revokes** (pure `classifyAdjustment`, unit-tested):

- **Approved full refund: revokes.** `type: full`, or (with no `type`, which Paddle's schema allows) every line item
  `full`. Pending refunds wait: most live refunds need Paddle's approval, which arrives as `adjustment.updated`. A
  rejected one never moved money.
- **Partial refund: records only.** Decision: in practice it's a goodwill amount or a price correction, and which seat
  of a multi-seat purchase an amount stands for is unknowable. A human who reads it as "two of five seats returned" has
  no per-seat revoke today; that'd be new work.
- **Approved chargeback, full or partial: revokes.** Decision: the buyer took the money back through their bank instead
  of asking, which isn't the goodwill case.
- **`chargeback_warning`, credits, and every `_reverse`: record only.** A warning moves no money yet. A
  `chargeback_reverse` (Paddle won the dispute) does NOT reinstate: revocation is one-way everywhere, reinstating means
  minting a new license by hand. Paddle then marks the original chargeback `reversed`, so the listing shows it with
  `revokes: false` beside a revoked license, which is the cue for that human.

**Idempotent by construction:** the adjustment is upserted on its `adj_` id and only moves forward in Paddle's own
`updated_at` order (retries are independent, so `created` can land after `updated`); `revokePaddleLicense` keeps the
first `revoked_at`; the KV deletes run on every delivery, so a retry finishes a delivery that died between the two.

**A refund can beat its own fulfillment.** If the purchase has no ledger row yet (its `transaction.completed` still
being retried), the revocation inserts one, born revoked. The late fulfillment then classifies it `revoked` and mails
nothing. An adjustment that doesn't revoke never creates a row.

**Admin-visible record:** `license_adjustments` (migration `0021`) keeps every adjustment at its latest status, with
Paddle's reason and amount. `GET /admin/licenses` returns each beside its license (`adjustments`, with a computed
`revokes`), plus `unmatchedAdjustments` for transactions no row describes (a pre-ledger purchase). Paddle's dashboard
stays the source for the money itself.

**Paddle setup:** both notification destinations (live and sandbox) must subscribe to `adjustment.created` and
`adjustment.updated` next to `transaction.completed`. Without them nothing arrives and nothing fails.

**Validation error granularity:** `paddle-api.ts` throws `PaddleApiError` on network/5xx errors and returns `null` on
404 (transaction not found). That's what lets `/validate` answer 200-invalid versus 502-upstream, and the desktop app
keep a valid cached "active" through a transient Paddle outage instead of overwriting it with "invalid".

**Activation counter:** `/activate` increments a KV counter at `_meta:activation_count` on each successful activation,
read by `/admin/stats`. Read-then-write, so it races under concurrent activations (KV has no atomic increment); the
count is approximate by design. It starts from zero when deployed; initialize via the CF API if a historical count is
needed.

## Device tracking (fair use)

On each `/validate` call with a `deviceId`, the server tracks the device in KV (`devices:{seatTransactionId}`) and logs
to Analytics Engine (binding `DEVICE_COUNTS`, dataset `cmdr_device_counts`). Devices older than 90 days are pruned on
each write. If 6+ devices are active and no alert was sent in the past 30 days, an internal email goes to
`legal@getcmdr.com` via Resend. The KV value stores a `DeviceSet` of device hashes → last-seen timestamps plus an
optional `lastAlertedAt`. Tracking is per SEAT: each seat in a multi-seat purchase has its own transaction id and its
own 6-device allowance.

**Decision: no hard enforcement of device limits.** The server never rejects a validation because of device count.
Suspension is a manual decision after human review; the goal is detecting obvious key sharing (one key on 6+ devices),
not restricting legitimate power users. The threshold is 6 because 3-4 Macs is normal, 5 is plausible, 6 is hard to
explain as one person, and it isn't published in the ToS to avoid gaming.

## Decisions

**`PADDLE_ENVIRONMENT` controls sandbox versus live routing**, rather than inferring from transaction ids. Both
environments use the same `txn_` prefix, so an explicit env var is the only unambiguous signal. `wrangler.toml` defaults
to `"sandbox"` for local dev; the deployed worker overrides to `"live"` via a wrangler secret.

**Price IDs live in env vars (`PRICE_ID_*`)**, not code: sandbox and live Paddle accounts have different ids for the
same products. `.dev.vars` carries sandbox ids, wrangler secrets carry live ids.

**Paddle as Merchant of Record** (not Stripe, Gumroad, LemonSqueezy, or Polar): all-inclusive pricing (5% +
$0.50, no
hidden non-US or EU payout fees), aggregate monthly payouts (one invoice for the accountant instead of per-transaction),
global VAT/GST calculation and remittance, established reputation (Sketch, etc.). On a $29
sale: $1.95 fee → $27.05 net. At 30k sales that saves ~$7k/year versus LemonSqueezy. Stripe was rejected because a solo
dev handling VAT in 27+ EU countries is impractical (Stripe is a payment processor, not an MoR).

**Commercial prices use `external` tax mode**, so commercial customers pay tax on top of the listed price. Configured
per-price in the Paddle dashboard (both sandbox and live).

## Sandbox runbooks

**Expose the local worker via ngrok** (for Paddle sandbox webhooks):

```bash
ngrok http 8787 --url unsickerly-acclivitous-lala.ngrok-free.dev
```

The ngrok domain is stable across restarts, and the Paddle sandbox notification destination already points to
`https://unsickerly-acclivitous-lala.ngrok-free.dev/webhook/paddle`.

**Generate a test license key.** Point the mint script at the local worker:

```bash
node apps/api-server/scripts/mint-license.js --email test@example.com --org "Test Corp" \
  --note "local testing" --api http://localhost:8787
```

It returns a short code like `CMDR-ABCD-EFGH-1234`. Local `/validate` needs the local D1 to have the table, so run
`wrangler d1 migrations apply cmdr-telemetry --local` once. These keys validate for real (against the local ledger),
unlike a Paddle purchase; for an end-to-end run through Paddle itself, use the sandbox checkout flow described in
[testing Paddle checkout](../../README.md#testing-paddle-checkout).

Frontend counterpart: `apps/desktop/src/lib/licensing/CLAUDE.md`.
