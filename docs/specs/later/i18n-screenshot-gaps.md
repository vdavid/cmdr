# Translator screenshots: the gaps still open

The screenshot harness ships and is re-runnable (`pnpm i18n:shots`). This doc is only the structural half of coverage:
which catalog families still have no honest image, why each one resists capture, and what closing it would take.
Everything about how the harness works lives with the code and is not restated here:
`apps/desktop/src/lib/intl/messages/DETAILS.md` § Screenshots (the mechanism, the framing rules, direct vs
representative), `docs/guides/i18n.md` § Screenshots (the overview), `apps/desktop/src/lib/dialog-gallery/DETAILS.md` §
"Two more callers" (the registry-driven dialog pass), and `apps/desktop/test/e2e-playwright/DETAILS.md` (window ACLs,
overlay rules).

❌ **No absolute numbers live in this doc.** Counts, percentages, and the per-area ranking come from
`apps/desktop/src/lib/intl/messages/screenshots/coverage-report.md`, which every coupler run rewrites with a per-area
table and a per-surface review. Read it before acting on this list. Numerals written here went stale twice while the
analysis around them stayed true, which is the whole reason they're gone: a family's shape is durable, its key count
isn't.

Two things to know before reading a count from anywhere:

- **The percentage falls as the catalog grows, and that is fine.** Absolute coverage keeps climbing; the denominator
  climbs faster whenever a feature lands with a lot of copy (the translated menu bar arrived as a large block of
  permanently-native `menu.*` keys). Judge progress on the uncoupled COUNT per area, never on the headline percentage.
- **The report and the catalogs can disagree in both directions.** A key the catalogs carry an `@key.screenshot` for
  that a fresh coupling would not produce stays invisible: the warn-only `message-screenshots-fresh` check reads
  missing-or-stale couplings and stays green over a coupling the report no longer justifies
  (`apps/desktop/scripts/DETAILS.md` § "The capture guard" covers the deleted-surface half). A full `pnpm i18n:shots`
  clears both directions.

## 1. `settings.mediaIndex`: the panel body behind the master toggle

The biggest single cluster left, by a wide margin. `settings-indexing-image-indexing` IS captured, but with image
indexing OFF, so the shot carries only the handful of keys visible above the toggle and the whole panel body (the CLIP
model card and its download/delete states, the scope and chosen-folders editor, the importance-threshold slider with its
buckets and preview lines, per-volume rows, the progress and reclaim lines) never renders. `fileExplorer.imageIndex` is
the same story from the pane side.

Closing it means a capture pass that turns image indexing on and walks the panel's states, which is more staging than
any existing settings surface does. `apps/desktop/test/e2e-playwright/image-index-settings.spec.ts` already knows how to
reach several of them and is the place to start.

## 2. `fileExplorer.navigation`: the sidebar's status, error, and connection states

The pane volume chooser is captured, so the group headings are covered. Everything conditional on a state the capture
never stages is not, and it splits into four clusters:

- **Per-drive index status** (`driveIndex.*`), the largest of the four: the freshness tooltips, the coalesced-changes
  and unreadable-spots variants, the context menu's enable/rescan/disable/stop/forget items, the footer, and every
  refusal toast (disconnected, indexing-off, queued behind a scan). Each needs a specific index state on a specific
  drive.
- **SMB connection**: the direct-connect tooltips and their in-flight states, plus the saved-password prompt.
- **Favorites and reachability**: the favorites empty state, the rename/remove/reorder failure toasts, and the
  unreachable-location toast.
- **Disk space and hardware**: the retry ladder on a failed space fetch, and the USB negotiated-speed labels.

Each needs a real failure or a real device staged, which is why they are still open.

## 3. `askCmdr`: the surfaces that need an agent doing real work

The scripted fake LLM answers a message, which is what got the captured Ask Cmdr surfaces, but the rail slot's script in
`apps/desktop/src-tauri/src/agent/chat/session.rs` is a single `Say` turn, so no tool row ever renders. That leaves the
per-tool "doing / done" progress lines (`askCmdr.tool.*`) and the rename-undo affordance (`askCmdr.renameUndo.*`) as the
two largest uncoupled families in the catalog after `settings.mediaIndex`, plus the provider-failure copy
(`askCmdr.error.*`), the threads panel's management states (`askCmdr.sessions.*`), and the proactive agent's own
additions (`askCmdr.decision.*`, `askCmdr.wakeDigest.*`).

The pattern for closing it already exists one slot over: the WAKE slot has three scripts selected by
`test_mode::wake_fake_script`, one of which calls a tool, precisely so a spec can drive a tool-call path. The rail wants
the same shape rather than more staging, and a rename plan is the fullest script to give it.

❌ The consent-screenshot chore is NOT this doc's. `askCmdr.consent.*` carries hand-written couplings that a capture run
has never justified, and that item belongs to `docs/specs/later/ai/wake-loop-follow-ups.md`.

## 4. Settings sections and pages the capture never visits

Cheap, mechanical wins, all in `SETTINGS_SECTIONS` (`apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts`):

- `settings.summary`: the parent summary grids. The capture deliberately passes the full section PATH so it lands on
  real content instead of a parent grid, so no summary page is ever photographed. One extra surface per top-level parent
  covers all of them.
- `settings.archives`: the `behavior-archives` section exists in `SettingsContent.svelte` but has no row in the
  capture's section list.
- `settings.appearance` and `settings.tint`: the date/time format options, the custom-format editor with its placeholder
  help, and the tint names, all behind a disclosure or a picker on an otherwise-captured page.

## 5. The long tail

Ranked by the report, the recurring shapes are: `suggestedOps` (the whole agent suggested-operations panel, at zero),
the transfer dialog's scan-unresponsive and target-will-be-created variants, the error-reporter dialog, licensing's
error and section copy, file-operation validation messages, the settings pages for AI, Advanced, and Behavior, the FDA
onboarding step, the downloads shortcut row, and a cluster of pane states (`fileExplorer.pane`, `readOnly`, `clipboard`,
`errorPane`, `unreachable`) that only render when a pane is in trouble.

A few `queue.*` row states remain (the awaiting-answer tooltip, resume and its aria label, the toolbar's selected
count): states a two-operation queue never reaches, and not worth a dedicated surface on their own. The queue window,
the corner chip, and the failure toast each already have a surface in the capture report.

## What is not a gap

The native keys (`menu.*`, the window title, the already-running alert) are drawn by the OS, so no webview capture can
ever reach them. They are counted in their own column for exactly this reason, and their `@key` descriptions carry the
context instead. The rule against faking a screenshot for one lives in `docs/guides/i18n.md` § Screenshots.

## Before shipping a locale

The `@key.screenshotNote` text on every representative coupling is translator-facing first-draft copy generated from the
curated table in `apps/desktop/scripts/representative-screenshots.ts`. It is worth a human read before a locale ships,
per principle 4 (humans review human-facing copy).
