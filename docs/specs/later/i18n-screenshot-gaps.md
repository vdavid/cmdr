# Translator screenshots: the gaps still open

The screenshot harness ships and is re-runnable (`pnpm i18n:shots`); this doc is only the list of catalog families that
still have no honest image, ranked by how much a translator gains from closing them. Everything about how the harness
works lives with the code and is not restated here: `apps/desktop/src/lib/intl/messages/DETAILS.md` § Screenshots (the
mechanism, the framing rules, direct vs representative), `docs/guides/i18n.md` § Screenshots (the overview),
`apps/desktop/src/lib/dialog-gallery/DETAILS.md` § "Two more callers" (the registry-driven dialog pass), and
`apps/desktop/test/e2e-playwright/DETAILS.md` (window ACLs, overlay rules).

**The live numbers are generated, not written here.**
`apps/desktop/src/lib/intl/messages/screenshots/coverage-report.md` is rewritten by every coupler run with a per-area
table and a per-surface review. Read it before acting on this list; the ranking below can drift as the catalog grows,
and it has.

Where things stand, on two bases that differ by a little:

- **What a coupler run would produce today**: 2,101 of 2,989 renderable keys get a screenshot (70%), 1,200 direct and
  901 representative; 756 uncoupled, 132 native. The tracked `capture-report.json` holds 132 surfaces, none failed. This
  is the basis every count below uses.
- **What the catalogs claim right now**: 2,174 keys carry a committed `@key.screenshot` (73%), 898 of them with a
  `screenshotNote` (representative). That is report-to-catalog drift in both directions: four keys the report would
  couple and the catalog doesn't carry, and 77 the catalog carries that a fresh coupling would NOT produce (seven of the
  screenshot files those name are gone from the report entirely, a removed or renamed surface). Only the first four are
  visible: the warn-only `message-screenshots-fresh` check reads missing-or-stale couplings and stays green over a
  coupling the report no longer justifies (`apps/desktop/scripts/DETAILS.md` § "The capture guard" covers the
  deleted-surface half of this). A full `pnpm i18n:shots` clears both.

(Both recomputed 2026-08-20 from the tracked `capture-report.json` and today's `en/` catalogs, by calling the coupler's
pure coverage functions read-only.)

**Coverage percentage falls as the catalog grows, and that is fine.** Absolute coverage keeps climbing; the denominator
climbs faster whenever a feature lands with a lot of copy (the translated menu bar alone added 129 `menu.*` keys, all of
them permanently native). Judge progress on the uncoupled COUNT per area, never on the headline percentage.

## 1. `settings.mediaIndex`: 80 keys behind the master toggle

The biggest single cluster left. `settings-indexing-image-indexing` IS captured, but with image indexing OFF, so the
shot carries four keys and the whole panel body (the CLIP model card and its download/delete states, the scope and
chosen-folders editor, the importance-threshold slider with its five buckets and preview lines, per-volume rows, the
progress and reclaim lines) never renders. `fileExplorer.imageIndex` (12 keys) is the same story from the pane side.

Closing it means a capture pass that turns image indexing on and walks the panel's states, which is more staging than
any existing settings surface does: `image-index-settings.spec.ts` already knows how to reach several of them and is the
place to start.

## 2. `fileExplorer.navigation`: 55 keys in the sidebar's error and connection states

The pane volume chooser is captured, so the group headings and the favorites empty state are covered. What is not: the
SMB connection tooltips and their in-flight states (`connectingDirectly`, `connectingWithSavedPassword`,
`useSavedPasswordMessage`), the favorites failure toasts (rename, remove, reorder), and the unreachable-location toast.
Each needs a real failure staged, which is why they are still open.

## 3. `askCmdr`: 82 keys that need an agent doing real work

Two thirds of these are `askCmdr.tool.*` (29) and `askCmdr.renameUndo.*` (21): the per-tool "doing / done" progress
lines and the rename-undo affordance. The scripted fake LLM answers a message, which is what got the four captured Ask
Cmdr surfaces, but `scripted_fake_llm` in `src-tauri/src/commands/agent/chat.rs` scripts a single `Say` turn, so no tool
row ever renders. The fake already supports `ScriptedTurn::CallTools`, so closing this is mostly a richer script (a
rename plan is the fullest one) rather than more staging. The rest is `askCmdr.error.*` (11, provider failures) and
`askCmdr.sessions.*` (9, the threads panel's management states).

## 4. Settings sections and pages the capture never visits

Cheap, mechanical wins, all in `SETTINGS_SECTIONS` (`apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts`):

- `settings.summary` (17): the parent summary grids. The capture deliberately passes the full section PATH so it lands
  on real content instead of a parent grid, so no summary page is ever photographed. One extra surface per top-level
  parent covers all of them.
- `settings.archives` (15): the `behavior-archives` section exists in `SettingsContent.svelte` but has no row in the
  capture's section list.
- `settings.appearance` (24) and `settings.tint` (12): the date/time format options, the custom-format editor with its
  placeholder help, and the twelve tint names, all behind a disclosure or a picker on an otherwise-captured page.

## 5. The long tail

`fileOperations.transferDialog` (18: the scan-unresponsive and target-will-be-created variants), `errorReporter.dialog`
(13), `licensing.error` (12) and `licensing.section` (11), `fileOperations.validation` (11), `settings.ai` /
`settings.advanced` / `settings.network` (11, 11, 10), `suggestedOps` (29, the whole agent suggested-operations panel at
0%), and `indexing.enrich` / `indexing.rescan` (9, 8).

Six `queue.*` keys remain (the awaiting-answer status and its tooltip, resume and its aria label, the toolbar's selected
count, and the chip's scanning aria label): row states a two-operation queue never reaches. Not worth a dedicated
surface on their own. The queue window, the corner chip, and the failure toast each already have a surface in the
capture report (`queue`, `queue-empty`, `queue-failed`, `operation-chip`, `operation-failure`); anything further about
those three is the operation-queue visibility plan's business, not this list's.

## What is not a gap

The 132 native keys (`menu.*`, the window title, the already-running alert) are drawn by the OS, so no webview capture
can ever reach them. They are counted in their own column for exactly this reason, and their `@key` descriptions carry
the context instead. The rule against faking a screenshot for one lives in `docs/guides/i18n.md` § Screenshots.

## Before shipping a locale

The `@key.screenshotNote` text on every representative coupling is translator-facing first-draft copy generated from the
curated table in `apps/desktop/scripts/representative-screenshots.ts`. It is worth a human read before a locale ships,
per principle 4 (humans review human-facing copy).
