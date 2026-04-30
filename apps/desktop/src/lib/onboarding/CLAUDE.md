# Onboarding module

Handles the first-launch full disk access permission prompt.

## Key files

| File                          | Purpose                                                           |
| ----------------------------- | ----------------------------------------------------------------- |
| `FullDiskAccessPrompt.svelte` | Modal shown when FDA is not yet granted or was previously revoked |

## Behavior

The component renders inside `ModalDialog` and presents pros/cons plus step-by-step instructions for granting Full Disk
Access in macOS System Settings.

Two actions are available:

- **Open System Settings** — calls `openPrivacySettings()` via IPC, then shows a follow-up hint to restart the app.
- **Deny** — saves `fullDiskAccessChoice: 'deny'` to settings, calls `startIndexingAfterFdaDecision()` so the indexer
  starts within this session, then calls `onComplete()` to dismiss.

## Indexer FDA gate

At app launch, the backend defers starting the drive indexer until the user has decided about Full Disk Access. The
recursive scan from `/` would otherwise trigger macOS native permission popups (iCloud, Photos, etc.) that stack on top
of this in-app FDA modal.

The gate fires when `fullDiskAccessChoice === 'notAskedYet'` AND the OS-level FDA check returns false. After the user
decides:

- **Deny** path: `FullDiskAccessPrompt.svelte` calls `startIndexingAfterFdaDecision()` so the indexer starts in the
  current session.
- **Allow** path: the user grants FDA in System Settings, then restarts the app. On next launch the OS check returns
  true, the gate passes, and the indexer auto-starts.

The Tauri command is idempotent — calling it when indexing is already running is a no-op. See
`src-tauri/src/indexing/CLAUDE.md` for the backend side.

The `wasRevoked` prop switches the copy from "first ask" to "revoked" framing.

## Where it is rendered

`routes/(main)/+page.svelte` decides whether to render the prompt by checking:

1. `checkFullDiskAccess()` IPC result — if FDA is already granted, sync setting to `'allow'` and skip.
2. If FDA is not granted:
   - `'notAskedYet'` → show first-time onboarding prompt.
   - `'allow'` (but FDA revoked) → show prompt with "revoked" framing.
   - `'deny'` → skip (user previously declined).

## Onboarding flag and deferred update toast

A separate `isOnboarded` boolean lives in `$lib/settings-store.ts` (default `false`). It exists so the auto-update
"restart to apply" toast doesn't fire during first-launch onboarding (the user just downloaded the app — they'd be
confused) nor stack on top of the FDA-revoked re-prompt.

`+page.svelte` calls `notifyOnboardingComplete()` from `$lib/updates/updater.svelte` in two places:

- `handleFdaComplete()` — fires whichever way the FDA prompt closes (Allow → restart hint, Deny → setting saved). The
  helper persists `isOnboarded: true` itself, so the page doesn't double-save.
- The `hasFda === true` branch — covers users who granted FDA before the flag existed. If `!settings.isOnboarded`, call
  the helper so they get unblocked too.

Around the same place where `showFdaPrompt = true` is set (both first-run and `wasRevoked`), `+page.svelte` also calls
`setFdaPromptShowing(true)` so the updater suppresses the toast while the modal is up. `handleFdaComplete()` flips it
back with `setFdaPromptShowing(false)`. See `$lib/updates/CLAUDE.md` § "Onboarding gating" for the updater side.

## Key decisions

**Decision**: Three-state setting (`notAskedYet` / `allow` / `deny`) instead of a boolean. **Why**: The app needs to
distinguish "never asked" (show first-time prompt), "granted but later revoked" (show revoked prompt with different
copy), and "user explicitly declined" (never ask again). A boolean would conflate "not asked" with "denied", losing the
ability to respect the user's explicit refusal.

**Decision**: No `onclose` prop on the ModalDialog (no x button, no Escape dismiss). **Why**: This is a blocking
onboarding prompt. If the user could dismiss it without choosing, the app would have no recorded preference and would
re-show the prompt on every launch. The user must explicitly click "Open System Settings" or "Deny" to proceed.

**Decision**: Post-click hint to restart manually instead of auto-detecting the grant. **Why**: Tauri has no API or
callback for when macOS System Settings grants FDA. Polling `checkFullDiskAccess()` would work but adds complexity and
may not detect the change instantly. A simple "restart the app" instruction is reliable and matches what other macOS
apps (VS Code, iTerm2) do.

## Key gotchas

- Tauri provides no callback for when the user finishes in System Preferences. The app cannot detect the grant
  automatically. The post-click hint tells the user to restart manually.
- Uses `dialogId="full-disk-access"` on `ModalDialog`, so MCP dialog tracking is automatic.

## Dependencies

- `$lib/tauri-commands` — `openPrivacySettings`
- `$lib/settings-store` — `saveSettings`
- `$lib/ui` — `ModalDialog`, `Button`
