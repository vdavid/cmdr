# What's new popup (frontend)

Post-update changelog summary. After Cmdr silently auto-updates, this popup shows the changelog slice between the
version the user last saw and the one running now, so the project's pace is visible. The backend
(`src-tauri/src/whats_new/`) parses the embedded `CHANGELOG.md` into a typed model; this frontend decides when to show
it and renders it.

## Module map

- `whats-new.ts`: PURE. `decideWhatsNew(...)` returns `show` / `stamp` / `wait` / `none`, plus `compareVersions`
  (numeric semver compare). No `$state`, no IPC, fully unit-tested in `whats-new.test.ts`.
- `whats-new-trigger.svelte.ts`: the effectful layer. Owns `whatsNewState` (`$state`), reads/writes settings, fetches
  the slice over IPC, flips the dialog open. Exports `runWhatsNewStartupTrigger(...)` (auto path), `openWhatsNew()` (the
  manual M3 seam), and `closeWhatsNew()`.
- `WhatsNewDialog.svelte`: the soft `ModalDialog` (`dialogId: 'whats-new'`). Renders releases via `snarkdown`, the empty
  state, the "See full changelog" link, and the footer (opt-out + Close).

## Show-once semantics (the contract)

`whatsNew.lastSeenVersion` is the stamp. On startup, after settings load, `runWhatsNewStartupTrigger` resolves the
current version and acts on `decideWhatsNew`:

- **Fresh install** (no `lastSeenVersion`, NOT onboarded): silent stamp, never a popup. Also keeps every E2E run
  popup-free (fresh data dir → not onboarded).
- **Inaugural showcase** (no `lastSeenVersion`, onboarded): user updated INTO the feature, so show the current release
  only (`since=null, max=1`), then stamp. Disabled → stamp silently. Keyed on `isOnboarded`, NOT the version key alone:
  that flag is the only thing telling a fresh install from an existing user updating in.
- **Upgrade**: enabled → show `lastSeen < v <= current`, newest first, `max:5`, then stamp. Disabled → stamp silently.
- **Downgrade**: rewrite `lastSeenVersion` to current, no popup. **Unchanged**: nothing.

`isOnboarded` lives OUTSIDE the settings registry (`$lib/settings-store`), so the trigger can't read it via
`getSetting`; the caller (`routes/(main)/+page.svelte`) passes it in via `loadSettings()`.

## Gotchas

- **`wait` must NOT stamp.** When onboarding or another startup modal is up, a would-show returns `wait`: the page
  re-attempts on `handleWizardComplete` (mirrors the update-toast re-attempt in `updater.svelte.ts`). Stamping on `wait`
  would eat the changelog forever after, for example, a crash-report launch. Silent-stamp paths (fresh / downgrade /
  disabled) run regardless of modals.
- **An empty slice on an auto-show collapses to a silent stamp**, never an empty auto-popup. `WhatsNewRelease[]` can be
  empty even on a real `show` decision (every in-range release dropped backend-side). The empty STATE is only reachable
  via the manual reopen (`openWhatsNew()`, `allowEmpty: true`).
- **`compareVersions` is numeric per component.** A string compare orders `0.10.0` before `0.9.0` and misreads an
  upgrade as a downgrade.
- **`{@html}` in the dialog is trusted**: the content is our own committed `CHANGELOG.md` (parsed backend-side), same
  trust level as `FriendlyError`'s `md!`. Don't feed user input through it. The changelog is UI copy: fix bad entries in
  `CHANGELOG.md`, never add fixup logic here.

## Dev override

`CMDR_SIMULATE_UPDATE_FROM=0.22.0` makes a dev session behave as if it just updated from that version. The backend
surfaces it via `whatsNewDevOverride()`. When set, the trigger BYPASSES `decideWhatsNew`: it diffs from that version
(`getWhatsNew(v, 5)`), force-opens the dialog regardless of the setting / onboarding / modals, and does NOT stamp, so
every relaunch keeps showing it until the var is unset.

## Manual smoke checklist

1. In the dev `settings.json`, set `whatsNew.lastSeenVersion` to an old version (for example `0.1.0`), relaunch
   (`pnpm dev --worktree whats-new-popup`): the popup shows the latest five releases.
2. Click "Not interested in changelogs": the dialog closes, a toast fires, `whatsNew.showOnUpdate` flips to `false`.
   Relaunch with an old `lastSeenVersion`: no popup (silent stamp).
3. `CMDR_SIMULATE_UPDATE_FROM=0.20.0 pnpm dev`: the popup shows on every relaunch and never stamps.

The M3 manual entry (Help > What's new / palette) calls `openWhatsNew()`; the seam is ready, wired by M3.
