# Licensing

Everything money touches: the Paddle webhook that fulfills a purchase, `/activate`, `/validate`, and `/admin/generate`.
`licensing.ts` holds the routes; `license.ts` (short codes, key signing, `generateShortId`), `license-issuance.ts` (the
D1 fulfillment record), `paddle.ts` (HMAC verify, `constantTimeEqual`), `paddle-api.ts` (Paddle REST), and
`device-tracking.ts` (fair-use device sets) are its leaves.

## Must-knows

- **Sandbox and live never mix** (accounts, keys, price IDs, webhook secrets, notification targets):
  `PADDLE_ENVIRONMENT` routes. ❌ Never infer the environment from a transaction id, both use `txn_`.
- **`ED25519_PRIVATE_KEY` is per-environment for the same reason.** `.dev.vars` holds the DEV signer; production's
  lives only as a wrangler secret. ❌ Never copy the production key into `.dev.vars`: it mints licenses every shipped
  build accepts offline, and there's no revocation short of shipping a new binary. The desktop app picks the matching
  public key by build mode; rationale and rotation caveat in
  `apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signing keys.
- **One purchase yields ONE set of license codes, but the email may repeat.** `/webhook/paddle` claims the transaction
  in D1 (`license_issuance`) BEFORE any side effect, stores the codes before emailing, and marks `emailed_at` after. A
  delivery that loses the claim classifies the row (`classifyIssuance`) instead of issuing beside it. DETAILS §
  Fulfillment.
- **Issuance rows never expire.** An expiring marker is exactly how a late redelivery mints a second set of usable
  perpetual licenses. ❌ Don't add a TTL or a cleanup job.
- **A take-over is conditional** (`UPDATE ... WHERE claimed_at = <the value we read>`), so two deliveries finding the
  same stale claim can't both proceed.
- **`/validate` separates "Paddle says invalid" (200 + `status: "invalid"`) from "Paddle is unreachable" (502 +
  `upstream_error`).** The desktop app falls back to its cached status only on the 502, so collapsing the two would
  overwrite a valid "active" cache during a Paddle outage.
- **Device tracking never affects the validation response**: it's fire-and-forget, and the server never rejects a
  validation over device count. Alerts go to a human. DETAILS § Device tracking.
- **Paddle preserves `custom_data` key casing**, so it's `organizationName`, ❌ never `organization_name`.
- **`/admin/generate` takes the Paddle webhook secret as its bearer token**, unlike every other admin route (those take
  `ADMIN_API_TOKEN`).

Fulfillment states, webhook verification, the replay-tolerance gap, price-ID mapping, device sets, and the sandbox
runbooks: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
