# What's new popup (frontend) — details

Read [CLAUDE.md](CLAUDE.md) first for the module map and must-knows. This file holds the depth: the show-once decision
table, the manual entry, and the E2E seam.

## Menu + palette (manual entry)

The manual reopen is the `help.whatsNew` command (Help > What's new / command palette), `App`-scoped, no default
shortcut. Its handler in `routes/(main)/command-handlers/app-dialog-handlers.ts` calls `openWhatsNew()`, which fetches
the latest five releases (no lower bound), force-opens the dialog (empty state if nothing is displayable), and never
stamps `lastSeenVersion`. The native menu side lives in `src-tauri/src/menu/` (id `HELP_WHATS_NEW_ID`, placed above
"Send feedback…"); see that module's `DETAILS.md` for the menu order and the SF Symbol / mnemonic. `help.whatsNew` is in
`menuCommands` (`shortcuts-store.ts`) so a future custom binding syncs its accelerator to the menu.

## E2E seam

The auto-popup is driven once at boot by `maybeRunWhatsNew()` in `routes/(main)/+page.svelte`. Two facts make the E2E
path non-obvious:

- **E2E boots onboarded.** The FDA mock grants Full Disk Access, so `resolveOnboardingMount` marks the instance
  onboarded. With no `lastSeenVersion` yet, the inaugural-showcase rule would auto-open a popup at boot, which leaks
  into whichever spec runs first and trips the overlay leak guard in `fixtures.ts`. So `maybeRunWhatsNew()`
  early-returns under E2E mode unless called with `force: true`. This keeps every non-whats-new spec popup-free.
- **The spec drives the real auto path explicitly.** `whats-new.spec.ts` emits the E2E-gated `e2e-rerun-whats-new` event
  (handler in `+page.svelte`, gated on `getAppMode() === 'e2e'`). The handler seeds `isOnboarded` (via `saveSettings`)
  plus `whatsNew.lastSeenVersion` + `whatsNew.showOnUpdate`, then calls `maybeRunWhatsNew(true)` so the real
  `runWhatsNewStartupTrigger` runs (decide → fetch → open → stamp).

**The whats-new keys are seeded via `seedSettingForE2E` (cache + save, NO cross-window emit), not `setSetting`.** A
`setSetting` seed emits a `settings:changed` event that loops back to this same window (no sender-id guard in the
settings store) and re-applies the seeded value to the cache asynchronously. When that late echo carries the old `0.1.0`
seed and lands after the trigger has already stamped `0.26.0`, it reverts the cache, and the next save persists `0.1.0`.
The non-emitting seed sidesteps the race and matches production, where the seed comes from disk at boot, never a live
emit.

## Manual smoke checklist

1. In the dev `settings.json`, set `whatsNew.lastSeenVersion` to an old version (for example `0.1.0`), relaunch
   (`pnpm dev --worktree whats-new-popup`): the popup shows the latest five releases.
2. Click "Not interested in changelogs": the dialog closes, a toast fires, `whatsNew.showOnUpdate` flips to `false`.
   Relaunch with an old `lastSeenVersion`: no popup (silent stamp).
3. `CMDR_SIMULATE_UPDATE_FROM=0.20.0 pnpm dev`: the popup shows on every relaunch and never stamps.
