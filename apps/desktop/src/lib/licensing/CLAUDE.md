# Licensing module

Manages license validation, trial period, commercial reminders, and expiration modals. Licenses use Ed25519 signatures
for offline validation (no server call needed after activation).

## Key files

### Frontend

- `licensing-store.svelte.ts` — state management, validation trigger
- `LicenseKeyDialog.svelte` — license key entry UI
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

**Decision**: Activation does local signature verification first, then server validation. **Why**: If the key has an
invalid signature, there's no point hitting the server. Local-first catches typos and forgeries instantly. Server
validation then confirms expiration status and records the activation.

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
- **Activation system not yet implemented** — Self-service deactivation via UI.

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
