# Licensing details (backend)

Depth and rationale. `CLAUDE.md` holds the must-knows; the flows, decisions, and gotchas live here.

## Two-layer caching

1. In-memory `LICENSE_CACHE: Mutex<Option<LicenseInfo>>`: avoids re-parsing/verifying the Ed25519 signature on every
   call.
2. `license.json` via `tauri-plugin-store`: persists the last verified server answer across sessions. Keys:
   `license_key`, `license_short_code`, `cached_license_status`, `last_validation_timestamp`, `expiration_modal_shown`,
   `commercial_reminder_last_dismissed`, `license_clock_high_water`.

What the cached answer is worth as it ages: § Offline policy.

## Offline policy

`offline_policy::resolve_license_state` decides, from the signed key (`KeyTerms`), the cached answer, and the
clock-guarded time. The principle (David, 2026-10-05): **unreachability alone never downgrades a valid signed license.**

- **A cached server status wins first.** `invalid` → Personal (a revocation, or a key the server never knew). `expired`
  → Expired. Any status the app doesn't recognize → Personal. `active` or no answer yet → decide by type below.
- **Perpetual** → Commercial, forever. No age limit on the cached answer, no clock read.
- **Dated key** (`expiresAt` signed into the payload, which hand-issued dated licenses carry) → Commercial until that
  date, then Expired, even if a fresh answer said `active`.
- **Renewing subscription** (no date in the key) → Commercial until the later of (last reported period end + 30 days,
  `RENEWAL_GRACE_SECS`) and (last verified `active` + 30 days, `UNCONFIRMED_TERM_GRACE_SECS`), then Expired. The
  second term covers Paddle's `past_due` (reported `active` with a period end already behind it).
- **Time-limited with no known end** (a subscription key activated offline and never confirmed, or a dated key whose
  date doesn't parse) → Commercial for 30 days from activation, then Personal. The one case that needs the server, and
  only subscriptions (no longer sold) reach it.

The server is still asked every seven days (`VALIDATION_INTERVAL_SECS`), and that's how a revocation, expiry, or
renewal arrives.

Hostile cases, each deliberate:

- **Revoked license on a Mac offline for months**: stays Commercial until it next reaches the server, then drops. The
  price of never downgrading on unreachability. The license gates no feature, only the title and the reminder.
- **Firewalling `api.getcmdr.com`** keeps a revoked perpetual key Commercial forever. Same trade, same bound.
- **Clock set backwards**: time-limited licenses compare against `effective_now`, the later of the system clock and
  `license_clock_high_water` (bumped at most hourly, so a status check doesn't write the store every time). A verified
  answer resets it to the server's `signedAt`, which is also how a clock that ran ahead (expiring a license early)
  recovers once the clock is fixed. Perpetual licenses don't care.
- **Forged "revoked"** (a TLS-intercepting proxy, a squatter on a lapsed domain): can't be signed without the production
  key, so it's `Unverified` and changes nothing. § Signed validation answers.
- **Replayed answer**: bound to a fresh nonce and the transaction id, so it fails `WrongNonce` / `WrongTransaction`.
- **Leaked key**: works offline anywhere until revoked, and the revocation reaches only Macs that can reach the server.
  The fair-use device alert (`apps/api-server/src/licensing/DETAILS.md` § Device tracking) is how a leak gets noticed.
- **Refund, then offline**: the Mac keeps Commercial until it reaches the server. Online, a full refund or chargeback
  comes back as a signed `invalid` at the next check, every seat of the purchase at once
  (`apps/api-server/src/licensing/DETAILS.md` § Refunds).
- **Local tampering with `license.json`**: out of scope. The user owns the machine, and the license gates nothing
  (`docs/threat-model.md` § 7). The cache isn't re-verified on read for that reason.

## Signed validation answers

`/validate` answers with `signedAnswer: { payload, signature }` beside the legacy plain fields when the request carries
a `nonce` (32 lowercase hex, fresh per request, `new_nonce`). `payload` is base64 JSON with `transactionId`, `nonce`,
`status`, `type`, `organizationName`, `expiresAt`, and `signedAt`; `signature` is Ed25519 by the license signing key
over `cmdr-validation-answer-v1\n` + the payload bytes. `verify_validation_answer` checks the signature against
`PUBLIC_KEY_HEX`, then the transaction id, then the nonce. The app reads only the signed fields.

**Why the prefix**: a license key's signature covers the bare payload JSON (which starts with `{`), so with the prefix
neither signature can pass for the other, whatever a caller puts in the transaction id.

**Why sign at all when it's HTTPS**: the threats that matter for a "revoked" answer are the ones TLS doesn't stop: a
corporate TLS-inspection proxy with its own root CA, and, if the company ever stops, whoever registers the lapsed domain
next and gets a valid certificate for it.

**Deploy order**: the server change has to be live before an app that requires signatures ships. Against an older
server every answer is `Unsigned`, which keeps the cached status (and leaves new activations "pending").

## Activation flow (verify/commit split)

Split into two phases so invalid keys are never persisted.

```
Frontend: verifyLicense(input)
  |
  |-- is_short_code("CMDR-XXXX-XXXX-XXXX")?
  |     YES → POST /activate → get full crypto key
  |
  v
validate_license_key()      ← Ed25519 verify offline
return VerifyResult         ← LicenseInfo + full_key + short_code (nothing stored)

Frontend: validateLicenseWithServer(transactionId)
  |                                   ↑ passed explicitly since key isn't stored yet
  v
Server says active            → commitLicense(fullKey, shortCode) → persist + onSuccess
Server says expired          → commitLicense(fullKey, shortCode) → persist + show error
Server says invalid          → DON'T commit. Show error. Nothing stored.
Network error / unverified   → commitLicense(fullKey, shortCode) → persist + fallback
```

Pasting the full key (from the license email) skips `/activate`, so with no server at all the key verifies, commits on
the network-error path, and the offline policy carries it from there.

`commit_license` does: store to `license.json`, write initial `cached_license_status`, update `LICENSE_CACHE`.

- `VerifyResult` fields: `info` (LicenseInfo), `full_key`, `short_code`.
- `LicenseInfo` fields: `email`, `transaction_id`, `issued_at`, `organization_name`, `license_type`, `short_code`,
  `expires_at` (signed into a dated key; `commit_license` seeds the initial cache with it).
- The frontend uses `license_type` to construct a fallback `LicenseStatus` when the server is unavailable.

## Key patterns

- Key format: `base64(JSON).base64(signature)`, split on a single `.`.
- Public key embedded at compile time as hex in `verification.rs` (`PUBLIC_KEY_HEX`).

## Signing keys

Two Ed25519 pairs, one per environment, selected by the same `debug_assertions` gate that picks
`LICENSE_SERVER_URL` in `validation_client.rs`:

- **Dev**: private half in `apps/api-server/.dev.vars` (gitignored, mode 0600, developer machines only), public half in
  the `debug_assertions` arm of `PUBLIC_KEY_HEX`. Signs the licenses a local `wrangler dev` on `localhost:8787` mints.
- **Production**: private half is a Cloudflare Worker secret, public half in the `not(debug_assertions)` arm. Signs
  what `api.getcmdr.com` mints.

**Why they're split**: the two gates have to agree, because a build that talks to one server while trusting the other's
key rejects every license it's handed. Before the split there was one pair, so the dev config file held the production
signer, and anything that could read that file could mint a license valid on every shipped build. That's the same
reasoning as `apps/api-server/CLAUDE.md`'s "sandbox and live never mix", applied to the signing key.

`each_build_verifies_against_its_own_servers_signer` in `verification.rs` guards both directions and is written to
hold under `cargo test` and `cargo test --release`.

**Rotation**: regenerate with `cd apps/api-server && pnpm run generate-keys` and paste the public half into the matching
arm. Rotating the dev pair costs nothing. Rotating production invalidates every license already issued, because
`PUBLIC_KEY_HEX` is the only key a shipped binary trusts and there's no second-key transition path. Adding one (accept
old and new during a window) is the prerequisite for ever rotating production without reissuing.

## Key decisions

**Decision**: BSL 1.1 license model: free personal use, paid commercial ($59/year or $199 perpetual), converts to
AGPL-3.0 after 3 years.
**Why**: An earlier AGPL + trial model felt pushy for hobbyists (trial countdown, nagware, trivial bypass). BSL gives
friction-free personal use (no nags), clear commercial terms, and simpler enforcement (title bar shows license type).
"Source-available" positioning avoids confusing "open source but not really" messaging. Machine IDs aren't tracked; one
license works on unlimited personal machines.

**Decision**: Ed25519 offline verification with the public key compiled in, rather than server-side-only validation.
**Why**: A file manager must work offline. Network-required checks would degrade or nag on a plane or behind a
restrictive firewall. Offline crypto verification works instantly and permanently; server calls only learn a revocation,
an expiry, or a renewal.

**Decision**: Two-layer caching (in-memory `Mutex<Option<LicenseInfo>>` + on-disk `license.json`).
**Why**: Ed25519 verification is fast (~microseconds) but the call chain involves store I/O and JSON parsing.
`get_license_info` runs on every `get_app_status` check (window title, menu state, frontend polling). The in-memory
cache avoids repeated store reads; the on-disk cache persists server results so the app doesn't re-validate every launch.

**Decision** (David, 2026-10-05): a perpetual license verifies offline forever and drops to Personal only on a verified
revocation; a time-limited one expires by a date, never by silence. **Why**: a paying customer's license must not
depend on our server being up, or on the company existing (the continuity question every corporate buyer asks). The
old rule (30 days without a successful check → Personal, for every type) turned any outage into a downgrade. Full rules:
§ Offline policy.

**Decision**: 7-day server re-validation interval instead of checking every launch.
**Why**: Server calls are cheap but not free (network + startup latency). 7 days carries a revocation or a cancellation
promptly while avoiding a call on every launch.

**Decision**: Short codes (`CMDR-XXXX-XXXX-XXXX`) exchanged server-side for full crypto keys, AND the full key in the
license email. **Why**: short codes are human-friendly to type but too short to carry an Ed25519 signature, so they need
`/activate`. The email carries each seat's full key too ("Offline key"), so a new Mac can be activated with no server.
Text in the email beat a license-file attachment: the existing key field already accepts it (whitespace from line
wrapping is stripped), it survives forwarding and attachment-stripping mail filters, and it needs no new UI.

**Decision**: `LicenseActivationError` typed enum instead of `Result<_, String>` for activation errors.
**Why**: The frontend was pattern-matching English substrings to pick an error message. A tagged enum
(`#[serde(tag = "code")]`) serializes as `{ code: "badSignature" }` so the frontend can `switch` on the code. Same
pattern as `MtpConnectionError`. Lives in `verification.rs`, used by `validation_client.rs` too.

**Decision**: Verify/commit split: `verify_license_async` (read-only) + `commit_license` (persist), instead of one
`activate_license_internal` that does both.
**Why**: The old flow stored the key before server validation. If the server said "invalid" and the user force-quit (or
the app crashed), the invalid key persisted. Now verify-then-validate-then-commit means invalid keys never touch disk,
eliminating defensive `resetLicense()` cleanup in error handlers.

**Decision**: `VerifyResult` is a separate struct from `LicenseInfo`.
**Why**: `VerifyResult` carries `full_key` (needed to call `commit_license` later) and `short_code`. These shouldn't leak
to the frontend via `get_license_info`; they're only meaningful during activation. Keeping them separate means
`LicenseInfo` stays clean for displaying license details.

**Decision**: `validate_license_async` is single-flight with a failure cooldown.
**Why**: Concurrent validation triggers (multiple windows, repeated startup calls) used to stampede the server with
identical requests and spam one network-error warn per call. A static `tokio::sync::Mutex` serializes validations; after
acquiring it, periodic revalidation (`transaction_id == None`) short-circuits when another caller just validated
successfully (`!needs_validation`) or when the last attempt failed under 60 s ago (`LAST_FAILED_VALIDATION_AT`).
Explicit activation (`transaction_id == Some`) always goes through: the user is actively waiting, and the cooldown must
not block a retry after a transient failure. The network-error log lives in `validation_client.rs` only (info in debug
where `localhost:8787` is usually down, warn in release).

**Decision**: `validate_license_async` accepts an optional `transaction_id` parameter.
**Why**: During activation the key isn't stored yet, so the function can't read the transaction ID from the store; the
frontend passes it explicitly. For periodic re-validation the parameter is `None` and the function reads from the stored
license. Avoids storing the key just to read the transaction ID back.

**Decision**: `CMDR_MOCK_LICENSE` env var bypasses all license logic including server calls.
**Why**: License UX testing needs every state (personal, commercial, expired, with/without modals). Without mocking you'd
need real keys per variant and a running API server. The mock skips network entirely. It compiles into debug builds and
`playwright-e2e` builds: the i18n screenshot run photographs the license surfaces on the release-profile E2E binary.
Neither ships, so the variable can't unlock a user's install.

**Decision**: an E2E build never reaches the license server, mock or not. `activate_short_code` answers a
`NetworkError`, and `validate_with_server` answers `ValidationOutcome::NetworkError`, which keeps the cached status as a
real outage would. **Why**: an E2E build is a release build, so its `LICENSE_SERVER_URL` is production, and a spec that
typed a code or carried a stored key would send test traffic there.

**Decision**: `refresh_window_title` lives inside the three functions that write the cached status
(`write_cached_status_without_validation`, `update_cached_status`, `reset_license`), rather than at the call sites that
change a licence. **Why**: the OS-level title is a `set_title` call and stays wherever it was last put, while the in-app
title bar is reactive and follows the status by itself. Setting it only during `setup` left the Dock's window list,
Mission Control, and the Window menu saying "Personal use only" for the rest of the session after someone activated a
licence (reported by a beta user, 2026-09-17). Sitting in the writers means a new activation or expiry path can't
forget it; `setup` calls the same function, so there's one way to set this title.

## Gotchas

**Gotcha**: `should_show_commercial_reminder` initializes the timer on first call rather than showing immediately.
**Why**: On first launch the user hasn't evaluated the app. Showing "get a commercial license" immediately is a hostile
first impression. The 30-day timer starts silently so the reminder appears after a month of use.

**Gotcha**: `ValidationResponse` reads only `signedAnswer`; the plain `status` / `type` / ... beside it are for older
app versions. **Why**: trusting them would let anyone who can answer the request (§ Signed validation answers) revoke a
license.

**Gotcha**: `validate_with_server` returns `ValidationOutcome` (`Success` / `UpstreamError` / `NetworkError` /
`Unverified`), not `Option`.
**Why**: The API server returns HTTP 502 when it can't reach Paddle (upstream error) vs HTTP 200 with `status: "invalid"`
when Paddle actively says the transaction is unknown. Collapsing both into `None` made `validate_license_async` trust a
stale "invalid" from a transient Paddle outage and overwrite cached "active". Now `UpstreamError`, `NetworkError`, and
`Unverified` fall back to cached status without overwriting, while `Success` (a verified answer, even `invalid`) is
definitive and cached.

**Gotcha**: `validate_license_async` returns `Result<AppStatus, String>`, not bare `AppStatus`.
**Why**: The Tauri command must propagate network/upstream errors so the frontend distinguishes "server rejected the key"
(`Ok(Personal)`) from "couldn't reach the server" (`Err`). Without this, the frontend's catch never fires (Tauri's
`invoke` only throws on `Err`), and stale cached `Personal` is misread as a server rejection.

## Dependencies

- External: `ed25519-dalek`, `base64`, `reqwest`, `tauri_plugin_store`, `sha2`, `chrono` (date parsing), `rand`
  (nonces), `core-foundation` (macOS).
- Internal: none.
