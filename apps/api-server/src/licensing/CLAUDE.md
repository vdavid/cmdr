# Licensing

Everything money touches: the Paddle webhook (fulfill a purchase, revoke on a refund), `/activate`, `/validate`, and the
hand-issued licenses behind `/admin/generate` and `/admin/revoke`. `licensing.ts` holds the routes and mounts
`manual-licenses.ts` plus `admin-licenses.ts` (the dashboard's list, and its note editor). Leaves: `license.ts` (codes,
signing, id namespaces), `license-issuance.ts` (the D1 ledger), `refunds.ts` (refund and chargeback adjustments),
`license-backup.ts`, `paddle.ts` (HMAC verify), `paddle-api.ts`, and `device-tracking.ts`.

## Must-knows

- **❌ No Node globals anywhere the money path reaches.** The Worker runs without `nodejs_compat`, so `Buffer` and
  `process` don't exist in production, and a test in the node project sails past one: a `Buffer.from()` in the key
  encoder made every mint throw for real buyers while the suite stayed green. ESLint refuses them across the whole
  Worker at edit time; `production-runtime.test.ts` and `webhook-runtime.test.ts` run the real Worker in workerd to
  catch what lint structurally can't (a dependency), the second over the purchase path itself (Paddle and Resend stubbed
  at the socket). `../../DETAILS.md` § Test runtimes.
- **Sandbox and live never mix** (accounts, keys, price IDs, webhook secrets, notification targets):
  `PADDLE_ENVIRONMENT` routes. ❌ Never infer the environment from a transaction id, both use `txn_`.
- **`ED25519_PRIVATE_KEY` is per-environment for the same reason.** `.dev.vars` holds the DEV signer; production's lives
  only as a wrangler secret. ❌ Never copy the production key into `.dev.vars`: it mints licenses every shipped build
  accepts offline, and there's no revocation short of shipping a new binary. The desktop app picks the matching public
  key by build mode; rationale and rotation caveat in `apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signing keys.
- **One purchase yields ONE set of license codes, but the email may repeat.** `/webhook/paddle` claims the transaction
  in D1 (`license_issuance`) BEFORE any side effect, stores the codes before emailing, and marks `emailed_at` after. A
  delivery that loses the claim classifies the row (`classifyIssuance`) instead of issuing beside it. A renewal (any
  `subscription_*` origin) issues nothing. DETAILS § Fulfillment.
- **Issuance rows never expire.** An expiring marker is exactly how a late redelivery mints a second set of usable
  perpetual licenses. ❌ Don't add a TTL or a cleanup job.
- **A take-over is conditional** (`UPDATE ... WHERE claimed_at = <the value we read>`), so two deliveries finding the
  same stale claim can't both proceed.
- **`/validate` separates "Paddle says invalid" (200 + `status: "invalid"`) from "Paddle is unreachable" (502 +
  `upstream_error`).** Collapsing the two would revoke working licenses during a Paddle outage. The app trusts only the
  200's `signedAnswer` (signed over its nonce): DETAILS § Key formats.
- **Device tracking never affects the validation response**: it's fire-and-forget, and the server never rejects a
  validation over device count. Alerts go to a human. DETAILS § Device tracking.
- **Paddle preserves `custom_data` key casing**, so it's `organizationName`, ❌ never `organization_name`.
- **A license we hand out lives in our ledger, not in Paddle.** `/validate` dispatches on the id namespace: `txn_` asks
  Paddle, anything else resolves from `license_issuance` where `source = 'manual'`. For a `txn_` id the table only takes
  the license AWAY (`revoked_at`, set by a refund, DETAILS § Refunds); ❌ never let it grant one, a canceled
  subscription would keep validating. Minting refuses without a `note`. DETAILS § Manual licenses.
- **`/admin/licenses` reports OUR records, ❌ never Paddle's truth.** `active` on a `paddle` row means we fulfilled the
  purchase; whether the subscription still runs only Paddle knows. DETAILS § The licenses listing.

Fulfillment states, webhook verification and its replay window, price-ID mapping, device sets, and the sandbox runbooks:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
