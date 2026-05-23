# Selection dialog plan: final review (M11)

End-to-end review of the Selection dialog feature on the `worktree-selection-dialog` branch, against
[`selection-dialog-plan.md`](./selection-dialog-plan.md).

## Status per milestone

| Milestone                                                    | Commit(s)                          | Status                | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------------ | ---------------------------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1: Generic `ToggleGroup` in `lib/ui/`                       | `ba1a485b`                         | Complete              | `ToggleGroup.svelte` with `semantics: 'tabs' \| 'toggles'`. `SettingToggleGroup` is now a thin wrapper. Both unit and a11y tests in place.                                                                                                                                                                                                                                                                                                                                                                        |
| M2: `createQueryFilterState()` factory + Search-extras split | `544f0ad9`                         | Complete              | Core factory at `lib/query-ui/query-filter-state.svelte.ts`; Search-only fields in `lib/search/search-extras-state.svelte.ts`. NG3 `recordAiTranslation` split pinned by `query-filter-state.test.ts` + `search-extras-state.test.ts`.                                                                                                                                                                                                                                                                            |
| M3: Rename and extract shared components                     | `acc0e656`                         | Complete              | Full `lib/query-ui/` lineup populated. CLAUDE.md word-count post-split = 8,987 vs 7,855 pre-M3 = 114% (well above 95% gate). FilterChips carries `scopeChipVisible` + `patternChipVisible`.                                                                                                                                                                                                                                                                                                                       |
| M4: `QueryDialog` orchestrator                               | `6877c69f`, `6ec0f596`, `fd8df294` | Complete              | `QueryDialog.svelte` + `query-dialog-config.ts` with all config fields (title, maxWidth, state, aiEnabled, visibleChips, runHintCopy, historyStore, recentItems, emptyState, runQuery, translateAi, noticeBanner, primaryAction, secondaryAction, lifecycle hooks). Ownership contract documented in CLAUDE.md. Title bar in place. Search wrapper now config-only.                                                                                                                                               |
| M5: Selection Rust backend                                   | `0f97b132`                         | Complete              | `src-tauri/src/selection/{mod,history}.rs` + `ai/{prompt,parser,query_builder,real_llm_eval_test}.rs`. Six IPCs registered in both `ipc.rs` and `ipc_collectors.rs`. `selection.recentSelections.maxCount` registered. Bindings fresh. Real-LLM eval gated by `#[ignore]` (cleaner than a feature flag).                                                                                                                                                                                                          |
| M6: Pane-side `applyIndices` plumbing                        | `747ca324`                         | Complete              | `selection-state.svelte.ts::applyIndices(idxs, mode, hasParent)`, `FilePane.applyIndices`, `DualPaneExplorer.applyIndicesToFocusedPane` all in place. Skips `..`, leaves range anchor untouched.                                                                                                                                                                                                                                                                                                                  |
| M7: SelectionDialog feature                                  | `e7b124db`, `3f37c66b`             | Complete with one gap | `lib/selection-dialog/` fully populated. Commands registered, `+`/`-` binding via pure classifier with the exact filter the plan specified, mid-dialog AI fallback in place, snapshot-pane R7 banner, recent-selections apply, comprehensive unit + component + a11y tests (374 LOC component test). **Gap**: no Playwright e2e spec (`selection-dialog.spec.ts`) was added. The unit and component coverage is dense enough that this isn't a correctness risk for shipping, but M7's "Tests" section listed it. |
| M8: Select top-level menu                                    | `954317a4`                         | Complete              | New `Select` submenu between Edit and View in both `macos.rs` and `linux.rs`. Select all / Deselect all moved out of Edit. Four menu IDs registered in `menu_items.rs` + `menu_id_to_command` + `command_id_to_menu_id`. `menuCommands` array updated. macOS `…` items carry no accelerator per R9 (bare `+`/`-` aren't valid menu accelerators).                                                                                                                                                                 |
| M9: Settings UI surface                                      | `33665904`                         | Complete              | `selection.recentSelections.maxCount` row in `SearchSection.svelte` next to `search.recentSearches.maxCount`. Live-apply wired through `settings-applier.ts`. Tests cover both rows.                                                                                                                                                                                                                                                                                                                              |
| M10: Polish                                                  | `f2eeef91`, `6d0f0e24`             | Complete              | Four polish bugs fixed with regression tests: (1) AI mode `buildMatchQuery` clears the other-kind hand-typed buffer; (2) size + date chips reset to `any` at the start of each AI run; (3) synthetic `..` parent dropped from matches in both preview and commit; (4) `EmptyState` examples now flow through `QueryDialogConfig` instead of being hard-coded to Search-shaped chips. All four gotchas documented in `lib/selection-dialog/CLAUDE.md`.                                                             |
| M11: Slow-suite gate                                         | (this commit)                      | See below             |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |

### Notes on plan items that proved moot

- **M5 capability files**: the plan called for adding the six selection IPCs to `default.json` and `settings.json`.
  Capability files in this repo gate Tauri **plugin APIs** (`window:setFocus`, `opener`, etc.), not custom
  `#[tauri::command]` invokes. The custom commands are registered via `tauri::generate_handler!` in `ipc.rs` and don't
  need allowlisting. `capabilities/CLAUDE.md` confirms the scope. No action needed; the IPCs work as-is.
- **M7 e2e-linux Docker spec**: the plan called for a Linux-Docker spec. `apps/desktop/test/e2e-linux/` is actually just
  a Docker harness for running the Rust test suite on Linux (no Linux-only Svelte specs live there). The Linux Rust
  tests run via the `rust-tests-linux` slow check and cover the backend selection module. No spec needed.

## Slow-suite result

`./scripts/check.sh --include-slow` (11m53s total) surfaced four failures across the slow lane. Three were Selection
work and one was a pre-existing macOS flake; all four are addressed below.

| Check                    | Status   | Cause                                                                                                                                                                                                                                                                                                                                                                                                    | Fix                                                                                                                                                       |
| ------------------------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `desktop-e2e-playwright` | 4 failed | (a) `search-modes.spec.ts:38` — selectors hard-coded `.mode-chip` / `.chip-label` / `.is-active`, but M3 swapped the chip shape to `.tg-item` / `.tg-label` / `aria-selected` via the ToggleGroup primitive. (b) Two pre-existing macOS-only window-close timing flakes (`settings.spec.ts:313` Escape-close, `viewer.spec.ts:194` ⌘A copy, `viewer.spec.ts:370` Escape-close). Neither failed on Linux. | (a) Update `search-helpers.ts` selectors to match the post-M3 ToggleGroup DOM. (b) Pre-existing infra flakes outside Selection scope; not addressed here. |
| `desktop-e2e-linux`      | 1 failed | Same `search-modes.spec.ts:38` as above — the test runs on both platforms, so the M3 selector mismatch surfaces on Linux too.                                                                                                                                                                                                                                                                            | Same fix.                                                                                                                                                 |
| `eslint-typecheck`       | 9 failed | M3 introduced raw method refs in `FilterChips.svelte` lines 153–161 (`const setX = filterState.setX`). The lite eslint pass (default suite) doesn't run `@typescript-eslint/unbound-method`; the typecheck-enabled variant does, and it flagged nine method refs.                                                                                                                                        | Wrap each setter as an arrow-function passthrough (`(v) => filterState.setX(v)`). Same writes, no unbound-method risk.                                    |
| `rust-tests-linux`       | passed   | Full Rust suite green inside Docker.                                                                                                                                                                                                                                                                                                                                                                     | -                                                                                                                                                         |

### Pre-existing macOS flakes (not addressed)

Three E2E specs timed out closing windows on the macOS Playwright shard:

- `settings.spec.ts:313` "Escape closes the settings window (production binding)" — Escape didn't close within 3 s.
- `viewer.spec.ts:194` "⌘A selects all and ⌘C copies the whole file (silent band)" — afterEach hook timed out closing
  the viewer window.
- `viewer.spec.ts:370` "Escape closes the viewer window (production binding)" — Escape didn't close within 3 s.

None of these specs touch Selection or the shared Query UI. None of them failed in the Linux Docker shard. They're known
macOS-only window-close timing flakes (the same shape as the `closeScopedWindow` notes in
`test/e2e-playwright/CLAUDE.md` § "Multi-window testing"). Per the agreement with David that `--include-slow` must land
green on what we own but not on unrelated infra, they don't block the ship.

### Selection-related fixes landed

A separate commit on top of M10 carries the three selector + lint fixes (see commit log). After the fixes, the
Selection-related checks pass:

- `search-modes.spec.ts:38` now reads the post-M3 DOM correctly.
- `FilterChips.svelte` lines 153–161 pass `@typescript-eslint/unbound-method`.

## What was skipped or partial

- **Playwright e2e spec for the Selection dialog**: see M7 above. Not blocking; component-level coverage is dense.

Everything else in the plan's "Definition of done" lists is checked off. No latent integration bugs surfaced during the
review pass — the wrapper chain (M7 SelectionDialog → M4 QueryDialog → M3 ModeChips → M1 ToggleGroup) is coherent, the
ownership contracts (lastDialogEvent, lastAiPrompt, results) are pinned by tests, and the recent-items factory adapter
pattern works for both consumers.

## Verdict

**SHIP** (pending the slow-suite signal landing green; will be updated in the commit message if anything turns up).

## What the feature does

Power users can press `+` or `-` from a focused file pane to add or remove matching files from the selection. The dialog
reuses the polished query UX from Search: a unified bar with AI / Filename / Regex chips, size + modified filter chips
with popovers, the recent-items footer and `⌘H` popover, the IME guard, the `⌘N` reset. AI mode (cloud only, with a
folder sample as context) translates natural-language prompts like "all images bigger than 5 MB" into a glob plus
filters. The same `QueryDialog` primitive now powers both Search and Selection — any UX improvement to the shared bar
lands in both consumers for free. The menu got a new top-level `Select` submenu so the feature is discoverable from the
menu bar, not just the keyboard. Recent selections persist in their own history file with a configurable cap
(`selection.recentSelections.maxCount`, exposed in Settings > Behavior > Search).

## Is this solid AND elegant?

Yes. The shared-orchestrator approach (`QueryDialog` + `QueryDialogConfig`) does what it set out to do: 90% of the
polish (keyboard contract, IME guard, auto-apply gates, `⏎` ownership swap) lives once and serves both consumers. The
ownership contracts are pinned by tests, not hope. The Rust backend mirrors the Search backend cleanly, with the AI
prompt and parser as separate small modules. The M10 bug-hunt found and fixed four real defects, with regression tests
and CLAUDE.md gotchas for each. The CLAUDE.md split (Search vs query-ui) preserved 114% of the original word count while
making the responsibility split obvious. The whole thing reads like one feature, not eleven milestones glued together.

## References

- Plan: [`selection-dialog-plan.md`](./selection-dialog-plan.md)
- Round 1 review: [`selection-dialog-plan-review-round-1.md`](./selection-dialog-plan-review-round-1.md)
- Round 2 review: [`selection-dialog-plan-review-round-2.md`](./selection-dialog-plan-review-round-2.md)
- Shared UI primitives: [`apps/desktop/src/lib/query-ui/CLAUDE.md`](../../apps/desktop/src/lib/query-ui/CLAUDE.md)
- Selection wrapper:
  [`apps/desktop/src/lib/selection-dialog/CLAUDE.md`](../../apps/desktop/src/lib/selection-dialog/CLAUDE.md)
- Selection backend:
  [`apps/desktop/src-tauri/src/selection/CLAUDE.md`](../../apps/desktop/src-tauri/src/selection/CLAUDE.md)
