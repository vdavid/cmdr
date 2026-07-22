# What's new popup (frontend) — details

Read `CLAUDE.md` first for the module map and must-knows. This file holds the depth: the show-once decision table, the
manual entry, and the E2E seam.

## Show-once decision table

`whatsNew.lastSeenVersion` is the stamp. On startup, after settings load, `runWhatsNewStartupTrigger` resolves the
current version and acts on `decideWhatsNew`:

- **Fresh install** (no `lastSeenVersion`, NOT onboarded): silent stamp, never a popup. Also keeps every E2E run
  popup-free (fresh data dir → not onboarded).
- **Inaugural showcase** (no `lastSeenVersion`, onboarded): the user updated INTO the feature, so show the current
  release only (`since=null, max=1`), then stamp. Disabled → stamp silently. Keyed on `isOnboarded`, NOT the version key
  alone: that flag is the only thing telling a fresh install from an existing user updating in.
- **Upgrade**: enabled → show `lastSeen < v <= current`, newest first, `max:5`, then stamp. Disabled → stamp silently.
- **Downgrade**: rewrite `lastSeenVersion` to current, no popup. **Unchanged**: nothing.

`isOnboarded` lives OUTSIDE the settings registry (`$lib/settings-store`), so the trigger can't read it via
`getSetting`; the caller (`routes/(main)/+page.svelte`) passes it in via `loadSettings()`.

`compareVersions` compares numerically per component: a string compare would order `0.10.0` before `0.9.0` and misread
an upgrade as a downgrade.

## Lead rendering

The dialog renders each release's `lead` through `snarkdown` inside a `<div class="lead">` (NOT a `<p>`). A lead can be
a `**bold headline**` followed by a Markdown numbered list; snarkdown emits a block `<ol>`, which a `<p>` can't legally
contain (the browser force-closes the paragraph). CSS styles `.lead strong` (lifted to primary text, the part most
people read) and `.lead ol`. The list only renders because the backend `build_lead` preserves in-paragraph newlines; the
parse contract lives in `src-tauri/src/whats_new/DETAILS.md`.

## Dev override

`CMDR_SIMULATE_UPDATE_FROM=0.22.0` makes a dev session behave as if it just updated from that version. The backend
surfaces it via `whatsNewDevOverride()`. When set, the trigger BYPASSES `decideWhatsNew`: it diffs from that version
(`getWhatsNew(v, 5)`), force-opens the dialog regardless of the setting / onboarding / modals, and does NOT stamp, so
every relaunch keeps showing it until the var is unset.

## Menu + palette (manual entry)

The manual reopen is the `help.whatsNew` command (Help > What's new / command palette), `App`-scoped, no default
shortcut. Its handler in `routes/(main)/command-handlers/app-dialog-handlers.ts` calls `openWhatsNew()`, which fetches
the latest five releases (no lower bound), force-opens the dialog (empty state if nothing is displayable), and never
stamps `lastSeenVersion`. The native menu side lives in `src-tauri/src/menu/` (id `HELP_WHATS_NEW_ID`, placed above
"Send feedback…"); see that module's `DETAILS.md` for the menu order and the SF Symbol / mnemonic. `help.whatsNew` is in
`menuCommands` (`shortcuts-store.ts`) so a future custom binding syncs its accelerator to the menu.

A **second** native entry point opens the same popup: Cmdr > Changelog… (id `CHANGELOG_ID`, below "Check for updates…"),
mapped to the same `help.whatsNew` command, so both menu items open the identical latest-five slice. Details (SF Symbol,
the shared-command reverse-map note) in `src-tauri/src/menu/DETAILS.md`.

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

## i18n

The dialog's own chrome (title, empty state, links, footer, opt-out toast) lives in the `whatsNew.*` catalog
(`$lib/intl/messages/en/whatsNew.json`), resolved via `tString()`; `cmdr/no-raw-user-facing-string` is enforced on
`lib/whats-new/`. The release CONTENT (lead, section titles, entries) is NOT catalog copy: it's the committed
`CHANGELOG.md` parsed backend-side and rendered through `snarkdown`, so changelog wording is fixed in `CHANGELOG.md`,
not here. The title's apostrophe is the curly U+2019, kept byte-identical in the catalog value. Runtime rules:
[`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
