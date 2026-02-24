# Updates module

Auto-update checker and restart notification for the Cmdr desktop app.

## Key files

| File                        | Purpose                                                            |
| --------------------------- | ------------------------------------------------------------------ |
| `updater.svelte.ts`         | Module-level `$state` singleton, update check loop, download logic |
| `UpdateNotification.svelte` | Fixed-position toast shown when an update is ready to install      |

## Architecture

`startUpdateChecker()` is called once from `+layout.svelte` on app start. It:

1. Fires an immediate `checkForUpdates()` call.
2. Schedules a `setInterval` using `advanced.updateCheckInterval` from settings.
3. Listens for setting changes via `onSpecificSettingChange` — clears and re-creates the interval when the value
   changes.
4. Returns a cleanup function that `+layout.svelte` calls in `onDestroy`.

`checkForUpdates()` transitions the state machine: `idle → checking → downloading → ready`. If an update is found, it
calls `update.downloadAndInstall()` automatically — no user confirmation needed. The user is only asked at the `ready`
stage whether to restart now or later.

```
idle ──check()──► checking ──update found──► downloading ──done──► ready
  ▲                  │
  └──────error/no update
```

`UpdateNotification.svelte` reads `getUpdateState()` and renders the toast when `status === 'ready'`. Clicking "Later"
sets a local `$state` `dismissed` flag in `UpdateNotification.svelte` — it does NOT change `updateState.status`. The
status remains `ready` until the next check cycle resets it. The notification is just hidden locally (session-only, no
persistence).

## Key patterns and gotchas

- `.svelte.ts` extension is required because `$state` can only live in `.svelte` or `.svelte.ts` files.
- The update endpoint URL is configured in `tauri.conf.json`, not in TypeScript.
- The updater plugin is omitted entirely in CI builds (gated in `lib.rs` by a CI env var).
- No retry or backoff on error — the next interval fires a fresh attempt.
- Default interval: 60 minutes. Configurable in settings from 5 minutes to 24 hours.
- No tests exist — the module has hard dependencies on the Tauri updater plugin and the network.
- Cleanup is mandatory: the return value of `startUpdateChecker()` must be called in `onDestroy`.

## Dependencies

- `@tauri-apps/plugin-updater` — `check()`, `Update.downloadAndInstall()`
- `@tauri-apps/plugin-process` — `relaunch()`
- `@tauri-apps/api/app` — `getVersion()`
- `$lib/settings/settings-store` — `getSetting`, `onSpecificSettingChange`
- `$lib/logging/logger` — `getAppLogger` (logs via unified LogTape bridge)
