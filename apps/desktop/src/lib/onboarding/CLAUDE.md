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

- **Open System Settings** — re-runs `checkFullDiskAccess()` (so TCC has a fresh registration of the bundle and the Cmdr
  row appears in the FDA list), then calls `openPrivacySettings()` via IPC, then shows a follow-up hint to restart the
  app. The IPC deep-links straight to the Full Disk Access pane (not the Privacy category list).
- **Deny** — saves `fullDiskAccessChoice: 'deny'` to settings, calls `startIndexingAfterFdaDecision()` so the indexer
  starts within this session, then calls `onComplete()` to dismiss.

## FDA gate

Two things are gated on the FDA decision at app launch:

1. **Drive indexer** — recursive scan from `/` would touch iCloud, Photos, and other TCC-protected paths.
2. **Path-based icon fetches** in `volumes::list_locations` — `NSWorkspace.iconForFile:` resolution for `/Applications`,
   `~/Desktop`, `~/Documents`, `~/Downloads`, the iCloud root, and other cloud-storage paths reaches into adjacent TCC
   services. On a fresh launch with FDA off, this stacks 5–10 macOS native popups (MediaLibrary, AppData, Desktop,
   Documents, Downloads, ...) on top of this in-app FDA modal.

Both gates use the same predicate, exposed by `crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)`: pending iff
`fullDiskAccessChoice === 'notAskedYet'` AND the OS-level FDA check returns false. The runtime side reads a global atom
via `is_fda_pending_runtime()` so background-thread callers don't reload settings.

After the user decides:

- **Deny** path: `FullDiskAccessPrompt.svelte` calls `startIndexingAfterFdaDecision()`. The Tauri command clears the
  runtime gate, starts the MTP hotplug watcher, and starts the drive indexer. As the scan walks protected paths
  (`~/Downloads`, `~/Documents`, `~/Desktop`, ...), macOS fires one TCC popup per folder — those are the "individual
  Allow/Deny prompts" the user opted into by denying FDA. Folders the user denies stay unindexed; their size shows as
  `<dir>`. The command does NOT re-emit `volumes-changed`: that would refire per-folder prompts via NSWorkspace icon
  resolution on top of the scan's prompts, doubling the dialog count. Sidebar favorites stay icon-less until the next
  listing-driven refresh.
- **Allow** path: the user grants FDA in System Settings, then restarts the app. On next launch the OS check returns
  true, the gate is open, and both the indexer and the icon fetches run normally with no popups.

The Tauri command is idempotent. See `src-tauri/src/fda_gate.rs` for the gate, `src-tauri/src/volumes/CLAUDE.md` § "FDA
gate" for the icon-side rules, and `src-tauri/src/indexing/CLAUDE.md` § "Defer indexer auto-start" for the indexer side.

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
- **TCC's registration hook fires on `open()`, not `opendir()`.** A `read_dir` against a protected directory may be
  silently denied without ever adding the bundle to the Full Disk Access list — leaving the user with no row to toggle
  on. The probe in `permissions.rs` opens specific protected _files_ (`~/Library/Safari/Bookmarks.plist`,
  `~/Library/Mail/V10/MailData/Envelope Index`, `~/Library/Messages/chat.db`, etc.) and walks them in order until one
  returns either `Ok` or `PermissionDenied`. `NotFound` doesn't trigger TCC, so we keep walking. The component re-runs
  `checkFullDiskAccess()` right before `openPrivacySettings()` so the registration is fresh when the Settings pane
  loads.
- **Deep-link host changed in Ventura.** macOS 13+ uses `com.apple.settings.PrivacySecurity.extension`; older macOS uses
  `com.apple.preference.security`. Both anchor on `Privacy_AllFiles`. `open_privacy_settings` picks the right one via
  `get_macos_major_version`. The same version informs the modal copy: macOS 12 and older append new FDA entries at the
  end of the list (instead of alphabetical), so the "find Cmdr" instruction adjusts.
- **macOS 26 (Tahoe) FDA auto-add is broken.** Even with a notarized Developer ID build at `/Applications/Cmdr.app`, the
  kernel/sandbox can short-circuit `read()` denials on TCC-protected paths without ever consulting `tccd` — meaning Cmdr
  never enters the Full Disk Access list automatically. We mitigate by firing `mmap`, `NSData dataWithContentsOfFile:`,
  and `read_dir` of the parent in addition to `read()` on a denial (one of them may thread the needle on some Tahoe
  minor versions), but on `macOS 26.1+` even the `+`-button manual add has been reported broken for some users. The
  modal's "Tip" substep guides the user to the `+` workflow as the user-side fallback. Don't spend hours
  re-investigating: this is a documented OS regression, not an app bug. References:
  [Apple Developer Forums #809549](https://developer.apple.com/forums/thread/809549) (Tahoe 26.1 FDA),
  [Backrest issue #986](https://github.com/garethgeorge/backrest/issues/986) (FDA broken on Tahoe 26.1),
  [Apple Developer Forums #757768](https://developer.apple.com/forums/thread/757768) (Quinn confirms `open()` is the
  trigger pre-Tahoe).

## Dependencies

- `$lib/tauri-commands` — `checkFullDiskAccess`, `getMacosMajorVersion`, `openPrivacySettings`
- `$lib/settings-store` — `saveSettings`
- `$lib/ui` — `ModalDialog`, `Button`
