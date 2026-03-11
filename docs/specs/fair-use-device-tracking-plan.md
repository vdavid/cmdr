# Fair use: device tracking for license abuse detection

## Intention

License keys are currently device-agnostic. The validation call sends only a `transactionId` and returns subscription
status. There's no way to detect if a single key is being shared across dozens of machines. We want to detect obvious
abuse (one key on 6+ active devices) while staying true to our privacy-first stance.

The goal is NOT to restrict legitimate power users with several Macs. It's to catch keys being passed around an office.
We don't enforce a hard limit in code. We monitor, alert ourselves, and reach out to the customer.

## Current state

- **Rust app** (`validation_client.rs`): sends `{ transactionId }` to `POST /validate`. No device identifier.
- **License server** (`index.ts`): stateless read-only lookup against Paddle API. No storage on validation.
- **Privacy policy**: says "no telemetry" and "only license validation and update checks." Also says "The desktop app
  has **no telemetry**" which needs clarification (device hash for license enforcement isn't usage telemetry, but the
  blanket statement could feel contradictory).
- **Terms of service**: says "don't share your key" but no fair use clause or enforcement mechanism.
- **KV storage**: only short codes and idempotency keys. No per-key device tracking.

## Plan

### 1. Terms of service update (section 2 + section 3)

**Section 2** ("What the license grants you"), change "unlimited machines" bullet:

```
The right to install Cmdr on your own devices, subject to the fair use guideline below
```

**Section 3** ("Acceptable use"), add a new bullet after the existing "share your license key" bullet:

```html
<li>
    <strong>Fair use:</strong> Each license is meant for the single person using it, on their own
    devices. We don't set a hard limit on how many machines you can use, but we do keep an eye on the
    number of distinct devices per key. If we spot something that looks like key sharing rather than one
    person with a few computers, we'll reach out first. If the situation isn't resolved, we may suspend
    the key.
</li>
```

### 2. Privacy policy update

Two changes in the "In the desktop app" section:

**Update the license key bullet** to mention the device identifier:

```html
<li>
    <strong>License key</strong>: stored locally on your machine. The app verifies it cryptographically
    offline, and periodically checks our server to see if your subscription has expired or been extended.
    During these checks, we also send a hashed device identifier so we can detect key sharing. We don't
    use this to track you or your activity, only to count distinct devices per license.
</li>
```

**Clarify the "no telemetry" paragraph** that follows. Change from:

```
The desktop app has no telemetry—it doesn't report usage statistics, feature usage, or anything else back to us.
```

To:

```
The desktop app has no usage telemetry—it doesn't report which features you use, how often you use them,
or anything like that. The only network calls it makes are license validation (including the device
identifier mentioned above) and checking for updates.
```

**Why:** The current blanket "no telemetry" could feel contradictory once we add the device hash. "No usage telemetry"
is more precise and still conveys the same spirit — we're not watching what you do, we're only checking license status.

### 3. Rust app: generate and send a device ID

**File**: new helper in `apps/desktop/src-tauri/src/licensing/device_id.rs`

- On macOS: use IOKit FFI (`IOServiceGetMatchingService` + `IORegistryEntryCreateCFProperty`) to read
  `IOPlatformUUID`. This avoids spawning a subprocess, parsing undocumented `ioreg` output, and the risk of format
  changes across macOS versions. Use the `core-foundation` crate for CoreFoundation type handling.
- Salt and hash: `SHA-256("cmdr:" + uuid)`. The product-specific salt ensures the hash can't be correlated across
  products even if they use the same hardware UUID. One-way, can't be reversed to the hardware UUID.
- Format: `v1:<hex>` — the `v1:` prefix allows migrating to a different hashing scheme later without old and new
  hashes looking like different devices.
- Cache in memory (it won't change during a session).
- On Linux (future): read `/etc/machine-id`, same salt-and-hash approach.

**File**: `validation_client.rs`

- Add `device_id: Option<String>` to `ValidationRequest`.
- Populate it from the new helper before calling `/validate`.

### 4. License server: receive, store, and monitor device IDs

**File**: `index.ts`, `/validate` route

- Accept optional `deviceId` field in the request body.
- If present, resolve the per-seat transaction ID (each seat in a multi-seat purchase has its own ID like
  `txn_abc-2` — device tracking is per seat, not per purchase, so each seat gets its own 6-device allowance).
- Read the current device set from KV (`devices:{seatTransactionId}`).
- Add/update the device entry with the current timestamp.
- Prune entries older than 90 days (handles reinstalls, old machines).
- Write back to KV.
- If the count after pruning is >= 6, trigger alert (see section 5 below).

**KV key**: `devices:{seatTransactionId}` (the per-seat transaction ID, not the purchase-level one)
**KV value**: `{ [deviceHash: string]: string (ISO timestamp) }` (last seen per device)
**No TTL on the KV entry itself** — we prune stale devices on each write instead.
**KV race condition:** The read-modify-write on the device set isn't atomic. Two devices validating the same key
simultaneously could lose a write. This is acceptable — validation happens every 7 days per device, so races are
extremely unlikely, and a lost write just means one device shows up on the next cycle. Don't over-engineer this.

### 5. Monitoring and alerting

#### Analytics Engine (queryable history)

On every validation call that includes a `deviceId`, write a data point to a new Analytics Engine dataset (binding:
`DEVICE_COUNTS`, dataset: `cmdr_device_counts`):

```typescript
c.env.DEVICE_COUNTS.writeDataPoint({
    indexes: [seatTransactionId],
    blobs: [seatTransactionId, deviceId],
    doubles: [deviceCount], // count after pruning
})
```

**Why Analytics Engine:** It's the same pattern we already use for download tracking. It's append-only, cheap, and
queryable via the CF Analytics Engine SQL API. This lets us investigate patterns retroactively ("show me all keys that
have been above 6 for the past month") without building a dashboard. We can query it via CLI with agents handling the
API calls, or build a lightweight admin view later if needed.

**Provisioning:** Add the `DEVICE_COUNTS` Analytics Engine binding to `wrangler.toml` (same section as the existing
`DOWNLOADS` binding) with dataset `cmdr_device_counts`. See `scripts/setup-cf-infra.sh` for the existing pattern.

#### Email alert (immediate notification)

When a key first crosses the threshold of 6 devices, send an internal alert email via Resend to `legal@getcmdr.com`.
Simple HTML (not plain text) so key info can be bolded, transaction IDs and commands are monospace, and it's pleasant to
scan. No need for a fancy template — just well-structured HTML with inline styles. The email should include:

- Transaction ID (monospace, linked to Paddle dashboard)
- Current device count (bolded)
- Customer email (fetched from Paddle API — see note below)
- A short checklist of next steps (see escalation path below)

**Note on customer email:** The current `/validate` route only calls `getSubscriptionStatus()`, which doesn't return the
customer email. To include the email in the alert, make an additional `getCustomerDetails()` call to Paddle. Only do
this when the threshold is crossed (not on every validation), so the extra API call is rare. The customer ID is
available from the subscription/transaction data that `getSubscriptionStatus()` already fetches.

**Why alert at 6:** We want to catch abuse early. A power user with 3-4 Macs is normal. 5 is plausible. 6 active
devices in a 90-day window is hard to explain as one person, and we'd rather investigate a few false positives than miss
real abuse. We can adjust the threshold later based on real data.

**Why alert on first crossing (not sustained):** Speed matters more than precision here. If a key is posted online, we
want to know within one validation cycle (7 days), not two (14 days). False positives are cheap — we look at the data,
decide it's fine, and move on. A missed week of key sharing costs real revenue.

To avoid alert spam, store a `lastAlertedAt` timestamp alongside the device set in KV. Only re-alert if the last alert
was 30+ days ago. This prevents repeated emails for the same key while still catching cases where device count keeps
growing.

### 6. Escalation path

When we receive an alert:

1. **Investigate.** Query Analytics Engine to see the pattern. Is the device count growing, or did it spike once and
   stabilize? (Multi-seat purchases aren't a concern here — each seat gets its own transaction ID like `txn_abc-2`,
   and device tracking is per seat, so a 5-seat purchase gets 5 × 6 = 30 devices total, not 6.)
2. **Friendly email.** Send a personal (not automated) email from `support@getcmdr.com` to the customer. Tone: curious,
   not accusatory. Something like: "Hey, we noticed your Cmdr license is active on quite a few devices — more than we'd
   expect for one person. If your team needs seats, we'd love to help set that up. If something else is going on, let us
   know!" Offer a link to buy additional seats.
3. **Follow up.** If no response after ~2 weeks, send one more email, slightly firmer but still friendly.
4. **Suspend via Paddle.** If still no response or they confirm sharing without buying seats, cancel the subscription
   through Paddle. This is the last resort and should be rare.

**Why email directly first, then escalate to Paddle:** We own the customer relationship and control the tone. Paddle is
the billing layer, not the communication layer. A friendly first touch from us is more likely to convert a key-sharer
into a multi-seat buyer than a cold automated message from a payment processor.

## What we explicitly don't do

- **No hard enforcement in code.** The server never rejects a validation because of device count. It always returns the
  real Paddle subscription status. Suspension is a manual decision after human review.
- **No raw hardware IDs stored.** Only a salted SHA-256 hash (`cmdr:` prefix). Can't reverse it, can't correlate
  across products.
- **No new network calls.** The device ID piggybacks on the existing periodic validation call (every 7 days).
- **No published threshold.** The ToS says "we keep an eye on it" but doesn't say "6 devices." This avoids gaming.

## Effort estimate

| Component | Effort |
|-----------|--------|
| ToS wording (section 2 + 3) | 15 min |
| PP wording (one bullet) | 10 min |
| Rust `device_id.rs` (IOKit FFI + salted hash) + wire it into `ValidationRequest` | ~1.5 hours |
| License server KV logic (receive, store, prune, count, warn) | ~2 hours |
| Analytics Engine data point + binding in `wrangler.toml` | 30 min |
| Alert email via Resend (template + send logic) | ~1 hour |
| Tests (Rust unit test for hashing, server test for KV + alert logic) | ~1 hour |
| Manual E2E test | 30 min |
| **Total** | **~1 day** |

## Task list

### Milestone 1: Legal text

- [x] Update ToS section 2: change "unlimited machines" to reference fair use guideline
- [x] Update ToS section 3: add the fair use bullet after the "share your license key" bullet
- [x] Update ToS `lastUpdated` date
- [x] Update PP: add hashed device identifier mention to the license key bullet
- [x] Update PP: change "no telemetry" to "no usage telemetry" with clarified wording
- [x] Update PP `lastUpdated` date
- [x] Run `./scripts/check.sh --check website-prettier,website-eslint,website-build,html-validate`

### Milestone 2: Rust device ID

- [x] Create `apps/desktop/src-tauri/src/licensing/device_id.rs`: extract `IOPlatformUUID` via IOKit FFI, salt with `"cmdr:"`, SHA-256 hash, prefix with `v1:`, cache in memory
- [x] Add `core-foundation` crate dependency (check license with `cargo deny`)
- [x] Add `mod device_id` to `licensing/mod.rs`
- [x] Add `device_id: Option<String>` to `ValidationRequest` in `validation_client.rs`
- [x] Wire `get_device_id()` into `validate_with_server()` so it's sent on every validation call
- [x] Unit test: `device_id` returns a stable string matching `v1:<64-char hex>`
- [x] Unit test: `ValidationRequest` serialization includes `deviceId` field
- [x] Run `./scripts/check.sh --check rustfmt,clippy,rust-tests`

### Milestone 3: License server device tracking

- [x] Add `DEVICE_COUNTS` Analytics Engine binding to `wrangler.toml` (dataset: `cmdr_device_counts`)
- [x] Update `Bindings` type in `index.ts` with `DEVICE_COUNTS: AnalyticsEngineDataset`
- [x] In `/validate` route: accept optional `deviceId` from request body
- [x] Implement device set KV logic: read `devices:{seatTransactionId}`, add/update entry, prune >90 days, write back
- [x] Write Analytics Engine data point on every validation with `deviceId`
- [ ] Test: validation still works when `deviceId` is missing (backwards compatibility)
- [x] Test: device set KV logic (add, prune, count)
- [x] Run `./scripts/check.sh --check license-server-prettier,license-server-eslint,license-server-typecheck,license-server-tests`

### Milestone 4: Alert email

- [x] Extend `getSubscriptionStatus()` return type (or the validate route) to expose customer ID when available
- [x] Implement threshold check: if device count >= 6 and `lastAlertedAt` is >30 days ago (or absent), trigger alert
- [x] Fetch customer email via `getCustomerDetails()` only when alert is triggered
- [x] Build simple HTML alert email (inline styles, bolded count, monospace transaction ID linked to Paddle dashboard, next-steps checklist)
- [x] Send alert via Resend to `legal@getcmdr.com`
- [x] Store `lastAlertedAt` in the device set KV entry to suppress re-alerts for 30 days
- [x] Test: alert fires on first crossing of threshold
- [x] Test: alert is suppressed when `lastAlertedAt` is recent
- [x] Test: alert fires again after 30 days
- [x] Run `./scripts/check.sh --check license-server-prettier,license-server-eslint,license-server-typecheck,license-server-tests`

### Milestone 5: Verify and deploy

- [ ] Manual E2E: run the app locally with `CMDR_MOCK_LICENSE` disabled, verify `deviceId` appears in the validation request
- [ ] Manual E2E: hit `/validate` with a `deviceId`, verify KV entry is created and Analytics Engine data point is written
- [ ] Manual E2E: hit `/validate` with 6+ distinct `deviceId` values for the same transaction, verify alert email arrives
- [x] Update `apps/license-server/CLAUDE.md`: add device tracking to the data flow, routes table, and bindings
- [x] Update `apps/desktop/src-tauri/src/licensing/CLAUDE.md`: mention `device_id.rs` and the new field in `ValidationRequest`
- [ ] Run full check: `./scripts/check.sh`
