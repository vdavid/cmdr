# Licensing module

Manages license validation, trial period, commercial reminders, and expiration modals. Licenses use Ed25519 signatures
for offline validation (no server call needed after activation).

## Key files

### Frontend

- `licensing-store.svelte.ts` — state management, validation trigger
- `LicenseKeyDialog.svelte` — license key entry + details view (key-value display, "Use a different key" reset flow)
- `CommercialReminderModal.svelte` — 30-day reminder for personal/supporter users
- `ExpirationModal.svelte` — shown when commercial license expires
- `AboutWindow.svelte` — displays current license status

### Backend

- `mod.rs` — public API exports
- `verification.rs` — Ed25519 signature validation, license activation
- `app_status.rs` — `AppStatus` enum, status checks, window title logic
- `validation_client.rs` — HTTP client for server-side validation (subscriptions only)

## License types

- **Personal** — free forever. Shows "Personal use only" in title bar. Commercial reminder every 30 days.
- **Supporter** — $10 one-time. Same as Personal but with a badge in About window. Commercial reminder every 30 days.
- **Commercial subscription** — $59/year. Server validation every 7 days. 14-day grace on network failure.
- **Commercial perpetual** — $199 one-time. No periodic validation. 3 years of updates.
- **Expired** — subscription expired. Shows modal once, then reverts to Personal behavior.

## Key decisions

**Decision**: Ed25519 offline verification for all license types, server validation only for subscriptions. **Why**: A
file manager must work without internet. Perpetual and personal licenses validate purely offline via Ed25519 signature.
Subscriptions need periodic server checks (every 7 days) to detect cancellation, but get a 14-day grace period so
intermittent network issues don't disrupt paid users.

**Decision**: Activation uses a verify/commit split — `verifyLicense` (read-only) then `commitLicense` (persist), with
server validation in between. **Why**: The old flow stored the key before server validation. If the server rejected it
and the user force-quit, the invalid key persisted. Now `verifyLicense` checks the Ed25519 signature without writing
anything, `validateLicenseWithServer` checks with the server (passing `transactionId` explicitly since the key isn't
stored yet), and `commitLicense` only runs when we want to keep the key. Invalid keys never touch disk.

**Decision**: Expired commercial licenses revert to Personal behavior (not locked out). **Why**: The app is usable for
free (Personal license). Locking out a paying user whose subscription lapsed would be hostile — they'd lose access to
their file manager. Instead, the app quietly downgrades and shows a one-time modal suggesting renewal. The user can keep
working.

**Decision**: `licenseState` is a plain object (not `$state`) despite living in a `.svelte.ts` file. **Why**: The
licensing store is consumed by layout-level code that reads `cachedStatus` and `shouldShowModal` imperatively (not
reactively). Svelte runes would add reactivity overhead for state that only changes on explicit user actions (activate,
dismiss). The About window and modals read the cached value on mount.

## Gotchas

- **Mock mode only in debug builds** — `CMDR_MOCK_LICENSE` env var bypasses validation. Silently ignored in release.
- **Ed25519 public key embedded** — hardcoded in `verification.rs`. Must match license server's private key.
- **Commercial reminder timing** — tracked in `license.json` via `firstRunTimestamp`. Shows 30 days after first launch,
  then every 30 days.
- **Server validation grace period** — 14 days. After that, expired license shows modal on next launch.
- **Trial persistence via Keychain** — uses IOPlatformUUID (hashed). Survives reinstalls. Fresh trial on new Mac.
- **Self-service deactivation** — "Use a different key" in `LicenseKeyDialog` calls `resetLicense()`, which clears all
  license data (key, short code, cached status, validation timestamp) and reverts to Personal mode.
- **Commit writes initial cached status (without validation timestamp)** — `commit_license` writes
  `cached_license_status` so `get_app_status` returns the correct license type immediately, but deliberately does NOT
  write `last_validation_timestamp`. This way `needs_validation()` returns true and the frontend derives
  `pendingVerification` from `hasLicenseBeenValidated()`. Once a real server validation completes,
  `update_cached_status` writes the timestamp and pending state clears.
- **`handleActivate` uses verify/commit split** — calls `verifyLicense()` first (nothing stored), then
  `validateLicenseWithServer(transactionId)` passing the transaction ID explicitly, then decides whether to call
  `commitLicense()`. Four outcomes:
    1. Server confirms active (commercial/supporter) → `commitLicense()` + `onSuccess()`.
    2. Server says expired → `commitLicense()` + inline error with expiry date (key IS valid, just expired).
    3. Server says invalid (returns `personal` type) → DON'T commit. Nothing stored. Tracks `serverInvalidRetryCount`
       for escalating messaging. Cancel and X just close (no cleanup needed).
    4. Network error (`newStatus` is null) → `commitLicense()` + constructs a fallback `LicenseStatus` from
       `LicenseInfo` and calls `onSuccess()` with `pendingVerification` flag set.
- **`pendingVerification` flag** — tracked in `licensing-store.svelte.ts`. Derived from backend state on startup:
  `hasLicenseBeenValidated()` returns false when `last_validation_timestamp` is absent (license committed locally but
  never server-verified). Also set directly during activation when the network fallback path is used. Cleared when
  `triggerValidationIfNeeded` successfully completes. Survives app restarts because the backend state persists. When
  set, the validity row shows "Not yet verified" (yellow) with a 7-day hint.
- **Server-invalid detection (`isServerInvalid`)** — the details view checks if `existingLicense !== null` AND
  `getCachedStatus()?.type === 'personal'`. This is a safety net for the edge case where a key is stored but a periodic
  re-validation found it invalid. With the verify/commit split, this is even rarer — it only happens if a previously
  valid key is later rejected during a 7-day periodic re-validation.
- **Mailto links use `openExternalUrl`** — Tauri blocks raw `<a href="mailto:">` navigation. All mailto links use
  `onclick={handleEmailClick}` which calls `openExternalUrl()` (via `@tauri-apps/plugin-opener`) to open the system mail
  client. The email address is also shown as visible text with a Copy button (using `copyToClipboard`) so users can
  manually copy it.
- **Error display split** — the error message (red, `--color-error`) and help/support text (secondary,
  `--color-text-secondary`) are in separate `<p>` elements. The `errorHelpHint` state holds context-specific help text
  from `getFriendlyError()` for non-server-invalid errors (signature failures, network issues). `getFriendlyError` uses
  `parseActivationError(e)` to extract a typed `LicenseActivationError` with a `code` field (for example,
  `badSignature`, `networkError`) and switches on it, instead of pattern-matching English substrings.
- **License details view uses `LicenseInfo`, not `getCachedStatus()`** — org name and license type in the details view
  derive from `existingLicense` (the `LicenseInfo` payload), not from the cached server status. Only `validityText`
  (expiry dates) comes from `getCachedStatus()` since expiry is server-sourced.

## Development

**Run with mock license**:

```bash
CMDR_MOCK_LICENSE=commercial pnpm tauri dev
```

**Reset trial** (debug builds only):

```bash
security delete-generic-password -s "com.veszelovszki.cmdr" -a "trial-*"
```

**Test commercial reminder**: Set `firstRunTimestamp` in
`~/Library/Application Support/com.veszelovszki.cmdr/license.json` to 31 days ago.

**Generate a test license key**: See
[license server CLAUDE.md](../../../../apps/license-server/CLAUDE.md#generate-a-test-license-key).
