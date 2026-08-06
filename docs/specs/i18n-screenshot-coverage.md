# Screenshot coverage: every dialog a translator can be shown

**Status**: NOT STARTED. **Owner**: David. Baseline measured at `c1710d66c`.

Translators get a `@key.screenshot` per string so they can see where their words land. Coverage sits at **1549 / 2743
keys (56%)** and the capture run is not green: three of its four passes fail before they capture anything. This plan
closes the two gaps that cost the most, and makes a clean run the thing you get by default.

Area docs to read first: `apps/desktop/src/lib/intl/messages/DETAILS.md` § Screenshots, `docs/guides/i18n.md` §
Screenshots, `apps/desktop/test/e2e-playwright/CLAUDE.md` (overlay-closing rules, the `ensureAppReady` focus contract),
`apps/desktop/src/lib/dialog-gallery/DETAILS.md`.

## The problem

### 1. Three capture passes fail, so their surfaces never exist

`node scripts/i18n-capture.ts` runs four launches: a default pass, two license passes, and two FDA passes. At
`c1710d66c`, `license:perpetual`, `fda:notgranted`, and `fda:denied` all die in `ensureAppReady`
(`helpers/app-lifecycle.ts:35`) waiting 30 s for `.file-entry`, which never appears under those mocked launch modes.

This is a REGRESSION, not a standing gap: every pass ran green at `46bfaf7b1`, and fails at `63be2dc3d`. Nothing in CI
runs `i18n:shots`, so it went unnoticed for the ~90 commits in between. Bisect that range against the FDA/license mock
paths (`CMDR_MOCK_FDA`, `CMDR_MOCK_LICENSE`, both honored only in the debug-assertions capture build).

The visible cost is small today (`about-perpetual` is the one surface a green run would add that the report lacks) but
it will grow silently, and a harness that cannot go green teaches everyone to ignore its output.

### 2. 18 of 33 registered soft dialogs have no surface at all

`SOFT_DIALOG_REGISTRY` has 33 entries. The capture driver stages 15 of them by hand. The other 18 are invisible to
translators:

`acknowledgements`, `alert`, `archive-password`, `bulk-rename-review`, `crash-report`, `delete-ai-model`,
`drive-index-stale`, `mkdir-confirmation`, `mtp-permission`, `new-file-confirmation`, `operation-log`, `ptpcamerad`,
`rename-conflict`, `selection-add`, `selection-remove`, `transfer-confirmation`, `transfer-error`, `transfer-progress`.

Hand-staging each one is the wrong shape: it's 18 bespoke blocks, and dialog 34 will be missed the same way.

### 3. Whole windows and one big area are unvisited

- **The Transfers window** (`/queue` route, its own window): `queue` is 0% (19 keys). The driver never opens it. The
  `transfer-dialog` surface it does capture is the soft progress dialog, a different thing.
- **Ask Cmdr**: 140 uncoupled of 149 (6%), the largest single-area gap, and new since the last capture.
- **`operationLog`**: 37 keys, 0%.

Uncoupled keys ranked (from `coverage-report.md`): settings 227, fileExplorer 165, askCmdr 140, queryUi 122,
fileOperations 98, licensing 71, viewer 62.

## Approach

**Drive the gallery from its registry instead of hand-staging dialogs.** `DIALOG_GALLERY_ENTRIES`
(`src/lib/dialog-gallery/gallery-registry.ts`) already enumerates every registered soft dialog with its reviewable
states, and `debug-open-gallery-dialog` (`routes/(main)/listener-setup.ts:434`) opens any `(dialogId, stateId)` with
fixtures wired. A loop over the registry replaces 18 bespoke blocks, and the existing `dialog-gallery-coverage` check
already fails when a dialog is missing from the registry, so dialog 34 gets a screenshot the day it gets a gallery row.

Two things the loop must handle honestly:

- **`hostWindow`**: gallery rows render over the MAIN window even when the real host is `settings` or `viewer`, so those
  captures show a backdrop the dialog never has in production. That is what `@key.screenshotNote` is for; say it.
- **`not-triggerable` rows** have no states, and `store-seeded` / `app-command` / `event-seeded` rows open by a path
  other than fixture props. The registry declares which; the loop reads it rather than guessing.

## Milestones

1. **Green the harness.** Bisect `46bfaf7b1..63be2dc3d` for what broke the FDA/license passes; fix; confirm four passes
   green and `capture-failed.json` empty. Everything else is measured against a run that finishes.
2. **Registry-driven soft dialogs.** One loop, the 18 missing dialogs captured, notes carrying the `hostWindow` caveat.
3. **The Transfers window.** A separate-window pass like the existing `shortcuts` one (note: `shortcuts` is skipped for
   a tauri-playwright per-window eval hang; check whether `queue` hits the same wall before committing to it).
4. **Ask Cmdr.** Real staging work: the rail, the chat, and the consent screen. Biggest key win, most staging effort.

Re-run `pnpm i18n:shots` and commit the regenerated artifacts at each milestone; coverage numbers in this doc are the
baseline to beat.

## Gotchas already paid for

- **The capture build ignores a warm `target/`.** It passes `--config profile.release.debug-assertions=true`, which
  rehashes every dependency fingerprint, so the whole graph rebuilds from `libc` up (~15 min) no matter what is cached.
  Budget for it; don't be surprised by it.
- **Never point a representative mapping at a screenshot the run doesn't produce.** When Settings > AI split into three
  surfaces, `settings-ai.png` stopped existing and all 101 `ai.*` couplings silently vanished while the catalog kept the
  dangling refs. Auditing `REPRESENTATIVE_SCREENSHOTS` targets against the fresh report is a one-liner; do it after any
  surface rename.
- **Toasts nothing staged can strand the run.** The virtual MTP device announces itself on its own schedule; the spec
  sweeps toasts once more at the end for exactly this reason (`e2e-playwright/DETAILS.md` § Toast lifecycle).
- **`i18n:shots` masks its exit code if you pipe it.** `capture && couple`: a failed capture skips the coupler, and
  `| tail` reports tail's success. Read `capture-failed.json` to know what really happened.
