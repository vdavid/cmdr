# Activation flow: server validation + payload fallback

## Context

Two issues surfaced during the license dialog redesign:

1. **`LicenseInfo` lacks `license_type`** — the dialog's details view depends on `getCachedStatus()` for org name and
   license type, but that data comes from server validation. The Ed25519 payload already contains both fields; they're
   just dropped when constructing `LicenseInfo`.

2. **No handling of "Ed25519 valid but server says expired/invalid"** — the dialog blindly calls `onSuccess()` after
   server validation regardless of the response, so expired subscriptions appear to activate successfully.

## Design decisions

**Activation = Ed25519 stores the key. Server validation determines operating status.** These are separate steps.
A cryptographically valid key proves we signed it. The server confirms whether the subscription is still active.

**Server response determines the dialog outcome at activation time:**
- `"active"` → success, show About window with commercial status
- `"expired"` → store key, show in-dialog message about expiry with renewal link
- `"invalid"` / network failure → store key, derive status from `LicenseInfo` payload fields (license type + org name
  from the signed key), call `onSuccess()` if the payload indicates a commercial license. The 7-day
  re-validation cycle will retry the server check later.

**The license details view should be self-contained from activation data** — show org name and license type from
`LicenseInfo` (payload), not from `getCachedStatus()`. The cached status adds server-sourced enrichment (expiry date)
but shouldn't be the sole source for fields that already exist in the signed payload.

**Frontend constructs fallback status, not the backend.** When the server can't confirm the license, the dialog builds
a status object from `LicenseInfo` fields (available after `activateLicense()` returns). This avoids a round-trip
through `loadLicenseStatus()` → `get_app_status` and removes the need for `activate_license_internal` to write an
initial cached status as a safety net. The backend activation path stays simple: verify + store, nothing else.

**Test keys come from Paddle sandbox, not `/admin/generate`.** Manual keys generated via `/admin/generate` use fake
transaction IDs that the server can't validate. Instead of adding KV-based lookup for manual keys, we use Paddle
sandbox checkout to generate real test keys that work end-to-end with `/validate`.

## Changes

### 1. Add `licenseType` to `LicenseInfo` (Rust + TypeScript)

The Rust `LicenseInfo` struct in `verification.rs` currently has: `email`, `transaction_id`, `issued_at`,
`organization_name`, `short_code`. Add `license_type: Option<String>` populated from `LicenseData.license_type`.

Mirror this in the TypeScript `LicenseInfo` interface in `licensing.ts`.

**Files:** `verification.rs`, `licensing.ts`

### 2. Use `LicenseInfo` fields in the details view

Change `LicenseKeyDialog.svelte` to derive org name and license type from `existingLicense` (which comes from
`getLicenseInfo()`) instead of from `getCachedStatus()`. Keep using `getCachedStatus()` only for the expiry date and
validity text (since expiry is server-sourced, not in the payload).

**File:** `LicenseKeyDialog.svelte`

### 3. Handle server validation outcome in `handleActivate`

`handleActivate` currently calls `activateLicense()` then `validateLicenseWithServer()` and blindly calls `onSuccess()`.
Change it to inspect the server response:

- **`status.type === 'commercial'`**: call `onSuccess()`.
- **`status.type === 'expired'`**: show inline error with expiry date and renewal link. Don't call `onSuccess()`.
- **`status.type === 'personal'` (server said "invalid")**: the server actively rejected the key. Construct a fallback
  status from `LicenseInfo` fields (license type + org name from the signed payload). If the payload indicates a
  commercial license, call `onSuccess()` — the key is cryptographically valid, the server just doesn't
  recognize it (for example, Paddle lag). Otherwise show a "couldn't verify" message.
- **Network error (catch block)**: same fallback — construct status from `LicenseInfo` payload. Show a brief note that
  server verification will happen later, but don't block the user. Different UX message from the "invalid" case:
  "Couldn't reach the license server — using your license data" (optimistic) vs. "Key not recognized by the server"
  (if we wanted to warn, though both fall back the same way).

The `activateLicense()` call returns the `LicenseInfo` (with the new `licenseType` field from change 1), so the
fallback data is already available — no backend round-trip needed.

**File:** `LicenseKeyDialog.svelte`

### 4. Remove initial cached status write from activation

The `update_cached_status` call in `activate_license_internal` (verification.rs:60-61) was a safety net so
`get_app_status` would return Commercial even if server validation failed. Now that the dialog constructs fallback
status from `LicenseInfo` on the frontend (change 3), this write is unnecessary. Remove it to keep the activation path
simple: verify signature → store key → return `LicenseInfo`.

Also remove the `app_status` import that becomes unused.

**File:** `verification.rs`

### 5. Update test key generation docs

Update the license server README to point developers toward Paddle sandbox checkout for generating test keys that work
end-to-end with `/validate`, instead of `/admin/generate` (which produces keys with fake transaction IDs that the
server can't validate).

**File:** `apps/license-server/README.md`

## Verification

1. Start the license server locally: `cd apps/license-server && pnpm dev`
2. Generate a test key via Paddle sandbox checkout (see license server README)
3. Start the desktop app: `pnpm dev`
4. Activate with the short code → should show commercial status in About window
5. Reset license ("Use a different key" → "Continue")
6. Re-activate with the same short code → should work, About shows commercial
7. Test without the license server running: activate with a full crypto key → should fall back to payload data, show
   commercial (via the frontend fallback, not a backend cached status write)
8. `./scripts/check.sh --check clippy --check svelte-check --check desktop-svelte-eslint --check desktop-svelte-prettier`
9. `cd apps/license-server && pnpm run typecheck && pnpm test`
