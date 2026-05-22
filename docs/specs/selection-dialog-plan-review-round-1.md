# Selection dialog plan: review round 1

Reviewer: fresh-eyes Opus agent. Reviewed the plan against the actual codebase (search dialog, search-state, file-explorer pane, command registry, command-dispatch, macOS/Linux menu builders, settings ToggleGroup, docs/testing.md, docs/style-guide.md, lib/search/CLAUDE.md, src-tauri/src/search/CLAUDE.md). The plan is well-thought-out and in good shape overall. The blockers below are mostly wrong assumptions about the current codebase that would send agents down the wrong path.

## 🔴 Blockers

### B1: The current `SearchModeChips.svelte` is NOT built on Ark UI's `ToggleGroup`

**Where**: Plan § "Mode chips: `lib/ui/ToggleGroup.svelte`" (line 268), and M1 § "Built on Ark UI's `ToggleGroup` (same as today's SettingToggleGroup, so a11y and keyboard nav come for free)."

**Issue**: The plan asserts both `SettingToggleGroup` and `SearchModeChips` already share an Ark UI foundation. Only `SettingToggleGroup` does (`@ark-ui/svelte/toggle-group`, 101 lines). `SearchModeChips.svelte` (246 lines, `apps/desktop/src/lib/search/SearchModeChips.svelte`) is a bespoke `role="tablist"` with hand-rolled arrow-key navigation, focus management, `chipButtons[]` ref array, ChipKey extended union (`SearchMode | 'content'`), and a per-chip `hint`/`badge`/`tooltipText` shape. Migrating it to Ark's `ToggleGroup` is not a "thin wrapper" task — it's a re-implementation that has to preserve: skip-Content focus motion; disabled chip with tooltip; the active-chip-becomes-`tabindex=0` rule (the rest are `tabindex=-1`); the AI-enabled visibility filter that hides the AI chip entirely when AI is off; the "is-coming-soon" softer styling distinct from `:disabled`.

Ark's `ToggleGroup.Item` doesn't ship `badge` / `hint` / `tooltip` slots, so the wrapper either renders a custom button inside `ToggleGroup.Item` (defeating much of the a11y win) or extends the option-cell template ad hoc.

**Fix**: 
1. Correct the plan's claim — only `SettingToggleGroup` uses Ark today.
2. Either commit to genuinely rebuilding `SearchModeChips` on top of Ark (acknowledge it's a bigger task than "thin wrapper") OR keep the bespoke implementation and don't pretend it's the same primitive. If you go with Ark, M1 needs a sub-bullet for "extend ToggleGroupItem with badge / hint / tooltip slots" with concrete CSS scoping and a11y check (axe rule: badge/hint mustn't break the button's accessible name).
3. Spell out the disabled-content-chip contract: `disabled` attribute, no shortcut, italic styling, tooltip — and confirm Ark's ToggleGroup honors all of those on its `ToggleGroup.Item`.

### B2: `SearchModeChips` is plain `<button role="tab">`, not a toggle-group — the abstraction picked is wrong

**Where**: Plan § "Naming" and § "Mode chips: `lib/ui/ToggleGroup.svelte`".

**Issue**: The plan renames `SearchModeChips` → `QueryModeToggleGroup` "built on top of `lib/ui/ToggleGroup.svelte`". But semantically, the search mode chips are a tab strip (single-select, exposes `role="tablist"`/`role="tab"`, drives a UI mode), not a toggle group (Ark's `ToggleGroup` defaults to `aria-pressed`/single-or-multi toggles). The current component uses `role="tablist"` + `aria-selected` deliberately. Pushing it onto `ToggleGroup` would either lose tablist semantics (a11y regression vs today) or make the generic `ToggleGroup` actually a `TabList` in disguise (which Settings doesn't use it as).

Also: `SettingToggleGroup` is genuinely a toggle group (selects a setting value among options). The two consumers have different a11y semantics. Forcing them into one primitive risks producing a Frankenstein primitive that satisfies neither.

**Fix**: Pick one of:
- (a) Keep `SearchModeChips` as a bespoke tab strip; don't generalize. Drop M1's `lib/ui/ToggleGroup` from scope. Settings keeps its own `SettingToggleGroup`. This is fine and saves a milestone.
- (b) Build two primitives in `lib/ui/`: `ToggleGroup` (Settings) and `TabStrip` or `ChipTabs` (Query). Different a11y contracts; that's the honest split. Plan accordingly.
- (c) If you really want one primitive, explicitly call it a tab strip with `role="tablist"`, and migrate Settings to it too (settings ToggleGroup becomes the wrapper, which IS conceptually a radio-as-tabs UI). Verify Settings' a11y tests don't regress.

David's call. But the current plan's framing — "they're the same primitive, settings doesn't use the extras" — papers over a real a11y semantics difference.

### B3: Plan's "search-results pane matching" contradicts the actual `SearchResultsView` data shape

**Where**: Plan § "Match semantics" (line 137-145): "regular panes return `entry.name`, `search-results://` panes return `entry.path`". Plan § "Goals" (line 44-45): "Selection works in `search-results://` snapshot panes too: matching runs against the full path... instead of the basename."

**Issue**: Per the existing `lib/file-explorer/CLAUDE.md` § "Search-results virtual volume" and search-fixup-brief item 15: in `SearchResultsView`, each adapted entry's `name` field IS the friendly full path (with `~` substitution for the home folder); `findItemIndex` matches on the basename of `path`. So the "full path" the user sees is actually `entry.name`, and `entry.path` is the canonical absolute path. The plan's accessor design (return `entry.path` for snapshot panes) gives the user a different path string than the one they're looking at on screen. That's a UX bug waiting to happen: type `~/Library/Logs/foo.log` and the matcher tries to match against `/Users/.../Library/Logs/foo.log` — works for `*` globs but breaks any anchored or substring-based exact-match expectation.

Also, FilePane's `selection-state` keys on the raw frontend index, but for snapshot panes the "entries" are the snapshot's entries; the plan's `applyIndices` and `selection-matching` need to agree on the same index space.

**Fix**: 
1. Pick one source string per pane kind and document it clearly: either the displayed `entry.name` (which is the friendly full path in snapshot panes) or the raw `entry.path` (absolute). Recommend `entry.name` so "what the user sees is what the user matches against."
2. Add this to R7's mitigation in the risk register: the hint should explicitly say what the matcher matches against (not just "full paths" vaguely).
3. Add a test case in M7's `selection-matching.test.ts` that pins the snapshot-pane accessor returns the friendly form.

### B4: Plan calls the model `gpt-5.5` repeatedly; treat that as a placeholder, not a contract

**Where**: M5 § "Definition of done" (line 729), M7 § Manual smoke (line 826), M10 (line 925), M11.

**Issue**: There's no `gpt-5.5` model in production at OpenAI as of 2026-05. The codebase has `gpt-5.5` only in test assertions as a "defense in depth" string-prefix match for hypothetical future models. Asking the executing agent to "verify against `gpt-5.5` end-to-end" sends them to a model that doesn't exist. They'll either pick a different model silently or get stuck.

**Fix**: Replace `gpt-5.5` with "the configured cloud AI model (David's Settings > AI > Cloud > OpenAI; whichever model is set there)" everywhere. R3's mitigation already says to configure via Settings; the model name to use should come from there, not be hard-coded in the plan.

### B5: `=` and `-` keyboard binding via `event.key === '+'` / `event.key === '-'` is internally inconsistent in the plan

**Where**: Plan § "Keyboard contract" (lines 366-376), M7 (lines 799-802), Risk register R5 (lines 991-994).

**Issue**: The plan says two different things in the same section:
- Line 368: "`=` (Shift+=, key `'+'`) | focused pane | Opens Selection dialog in `add` mode"  
- Line 372: "Both bind via `event.key === '+'` / `event.key === '-'` so non-QWERTY layouts that produce the same key event also work."

This conflates Shift+= (which fires `event.key === '+'`) with bare `=` (which fires `event.key === '='`). On QWERTY US layout: bare `=` fires `event.key === '='`; Shift+= fires `event.key === '+'`. They are DIFFERENT events. If we want bare `=` to open the dialog, we bind `event.key === '='`. If we want Shift+= (which the user perceives as typing `+`), we bind `event.key === '+'`.

Then the Decision log line 448: "Bare `=` and `-` shortcuts, no modifier." Implies bare `=`, not Shift+=. Most file managers (Total Commander, Double Commander) use `+` for "select all by pattern" — that's Shift+= on US, but also the bare `+` key on numeric keypads.

**Fix**: Decide explicitly which physical keystroke triggers the dialog:
- **Option A (Total Commander parity)**: bind `event.key === '+'` and `event.key === '-'`. On US QWERTY: Shift+= and bare `-`. On layouts where `+` is unshifted, just `+`. This matches TC's behavior; users coming from there expect it.
- **Option B (truly bare `=`)**: bind `event.key === '='` and `event.key === '-'`. The plan keeps calling this "bare `=`" so Option B is what the prose says, but the implementation snippet uses Option A.

Recommend Option A; rewrite the keyboard contract table accordingly: "`+` (Shift+= on US) | focused pane | Selection add" and "`-` (bare) | focused pane | Selection remove". Risk register R5's mitigation stands either way.

Also pin this with an explicit unit test that asserts the key-event filter.

### B6: M4 expects to shrink `SearchDialog.svelte` from 1377 → ~150 lines; the orchestration that doesn't fit in `QueryDialog` is way more than 150 lines

**Where**: M4 § "Definition of done" (line 666): "`SearchDialog.svelte` is ~150 lines (down from 1377), almost all config-building and Search-specific glue (index lifecycle, snapshot promotion, MCP)."

**Issue**: The current `SearchDialog.svelte` is 1377 lines. Search-specific glue alone includes: `prepareSearchIndex` / `releaseSearchIndex` lifecycle with the timer dance; the MCP `mcp-open-search-dialog` listener wiring; snapshot-store create/setLastAttemptId path; the recent-searches add-on-write hook (1 call site, but with the AI-prompt-vs-pattern logic); `parseSearchScope` IPC for the scope row; `excludeSystemDirs` exclusion handling; the `searchable-folder` fallback for "use current folder" when on a snapshot pane; the `showAllInMainWindow` builder; the `onNavigate` exit path with cross-volume guard; the `lastDialogEvent` event sequencer for the Enter ownership swap (the events are recorded inside the dialog's effect blocks, not by `QueryDialog`); the auto-mode-fallback when AI gets disabled mid-session; the AI-strip lifecycle effect.

If `QueryDialog` owns the auto-apply gate + IME guard + `deriveEnterAction` + `lastDialogEvent` + the recent-items wiring + the title bar, the Search wrapper is still going to be 400-600 lines, not 150. That's fine, but the plan's "down to ~150" target will lead the agent to inappropriately stuff Search-specific concerns into `QueryDialog` to hit the number.

**Fix**: Drop the line-count target; replace with "the consumer wrapper builds the config, registers consumer-specific lifecycle hooks (mount, destroy, secondary actions), and forwards. No orchestration logic lives in the wrapper."  Trust the code shape, not the line count.

Also: explicitly call out that `lastDialogEvent` is set by event sources (input change, cursor move, results arrive, query edit). Decide whose responsibility it is to write to it. If `QueryDialog` does it, then the wrapper's `runQuery` callback can't be allowed to update results without the dialog seeing it — the callback's return value must be the entries, and `QueryDialog` writes `lastDialogEvent = 'results-arrived'` after the resolve. Spec this in `QueryDialogConfig`.

### B7: M4 doesn't address `excludeSystemDirs` and `scope` cleanly — these are search-only fields embedded in shared state

**Where**: M2 § "What" (lines 502-516), M4 § `QueryDialogConfig`.

**Issue**: `search-state.svelte.ts` carries `scope` and `excludeSystemDirs` as part of the unified state. The factory `createQueryFilterState()` either keeps them (Selection's instance will have unused fields, with the implied "Are these meant to be used?" confusion for the next reader), drops them from the factory (then M2's "Search becomes a consumer" claim requires Search to extend the base shape with its own fields — non-trivial because `applyHistoryEntry`, `applyHistoryFilters`, `buildSearchQuery` all touch them), or splits the state into "core" + "search-specific add-ons" (cleanest, but a bigger refactor than the plan implies).

Same problem for `lastAiPattern` / `lastAiPatternKind` / `lastAiLabel`: Selection's AI translation result has a `pattern + kind` but no `label` (Selection's pane breadcrumb doesn't need a friendly label).

**Fix**: Spell out which path M2 takes. Recommend the third option: factor `createQueryFilterState({ defaults })` to expose ONLY the cross-consumer fields (query, mode, sizeFilter, dateFilter, results, cursor, lastAiPrompt, lastAiCaveat, hand-typed buffers, IME state, `lastDialogEvent`). Search-specific fields (`scope`, `excludeSystemDirs`, `lastAiLabel`, `lastAiPattern`) live in a separate `search-extras.svelte.ts` module that the Search wrapper composes onto its instance, OR are passed in via the consumer's prefill/build callbacks. Document the split in `lib/query-ui/CLAUDE.md`.

Also: `buildSearchQuery` is search-specific (returns `SearchQuery` for the Rust IPC). Don't put it on the shared factory; have Search keep it next to the Search wrapper. The matching helper for Selection is a different function (operates on in-memory entries, not a Rust IPC).

### B8: `recordAiTranslation` semantics make Selection's "matching mode" path ambiguous

**Where**: M2 (state factory), implicit in M5 (Selection AI translation).

**Issue**: Today's `recordAiTranslation` writes to `handTyped.regex` or `handTyped.filename` based on `kind`. For Search, that wiring makes sense (the user might switch from AI to filename to refine). For Selection, the plan says Selection's bar shows the AI-translated pattern via the Pattern chip the same way Search does. But Selection has only two modes vs Search's three (no `ai` content slot, etc.), and Selection's "switch out of AI to refine" UX isn't speced. Does Selection have a Filename and a Regex mode? The plan implicitly says yes (M2: "factory carries `defaultMode: 'filename'`"; § "Match semantics" line 138-145 mentions `mode: 'glob' | 'regex'`). Confirm.

**Fix**: In M7's `SelectionDialog.svelte` spec, explicitly state which modes Selection exposes (AI / Filename / Regex; no Content). Add an a11y test for the mode chip set. Add a test that switching AI→Filename in Selection hands over the AI's glob pattern. Pin in `lib/selection-dialog/CLAUDE.md`.

## 🟡 Important gaps

### G1: Missing capability file updates (the AGENTS.md "fail silently" rule)

**Where**: M5 / M7. No mention of `src-tauri/capabilities/{default,settings,viewer}.json`.

**Issue**: Per AGENTS.md: "Whenever you call a new Tauri API from a window, add the matching permission to that window's capability file." The new commands `translate_selection_query`, `get_recent_selections`, `add_recent_selection`, `remove_recent_selection`, `clear_recent_selections`, `apply_recent_selections_max_count` will be called from the main window (selection dialog) and possibly the settings window (live-apply). Without capability entries they fail silently with "not allowed."

**Fix**: Add to M5's checklist: "Update `src-tauri/capabilities/default.json` to allow the six new commands from the main window. Update `settings.json` capability to allow `apply_recent_selections_max_count` from the settings window. Confirm by opening the settings window and changing the cap."

### G2: Specta exclusion list

**Where**: M5 § IPC.

**Issue**: Per `lib/ipc/CLAUDE.md` (referenced from AGENTS.md): some commands are excluded from specta bindings (debug-only, etc.). The plan doesn't say whether the new selection commands should be auto-included; assume yes, but call it out so the agent doesn't accidentally add them to the exclusion list (or get confused if they're missing after `pnpm bindings:regen`).

**Fix**: Add to M5: "The new selection commands appear in `bindings.ts` after regen. They're NOT debug commands; no exclusion list entry needed. Call them via `commands.translateSelectionQuery(...)` and similar from `$lib/tauri-commands`."

### G3: `command-registry.ts` scope value not specified

**Where**: M7 § "What" (line 792): "with scope `'Main window/Selection dialog'` (or similar; align with the existing naming)".

**Issue**: `CommandScope` is a fixed union literal in `lib/commands/types.ts`. Adding `'Main window/Selection dialog'` requires extending the union. Existing selection commands use `'Main window/File list'`, which suggests the new "Select files" and "Deselect files" commands (which open a dialog from a focused pane) should use the same scope. The "or similar" hedging will cause inconsistency.

**Fix**: Specify scope: `'Main window/File list'` for `selection.selectFiles` and `selection.deselectFiles` (they're file-list-scoped commands that happen to open a dialog). Don't add a new scope.

### G4: Existing `selection.selectAll` already lives in Edit menu — moving it to Select breaks `menuCommands` shortcut sync

**Where**: M8 § "What" (line 850): "Remove the same Select all / Deselect all items from the Edit menu (where they live today)."

**Issue**: This is fine, but the plan needs to call out: `shortcuts-store.ts::menuCommands` registers the existing IDs as menu-bound; the `menu.rs` `command_id_to_menu_id` maps `selection.selectAll → SELECT_ALL_ID`. Both macOS (`macos.rs`) AND Linux (`linux.rs`) builders register `select_all_item` under the Edit menu. The plan says "edit `menu_structure.rs` (and the macOS builder)" but there are TWO platform builders — `macos.rs` line 130-145 and `linux.rs` line 89-95. Both must move the items.

**Fix**: In M8, list the actual files: `apps/desktop/src-tauri/src/menu/macos.rs` and `apps/desktop/src-tauri/src/menu/linux.rs`. The shared `menu_structure.rs` only houses `build_menu` (the platform dispatcher) and `build_context_menu`. Add explicit instructions: "(a) Add a new `Select` submenu in `macos.rs::build_menu_macos` between Edit and View, registering the four items there. (b) Same in `linux.rs::build_menu_linux`. (c) Register the new IDs in `menu_items.rs`. (d) Update `mod.rs::menu_id_to_command` and `command_id_to_menu_id` for the new IDs."

### G5: M8 changes accelerator on `selection.selectAll`, but its accelerator IS `Cmd+A` which doesn't move

**Where**: M8 § Menu structure (line 391): "Select all ⌘A" inside the new Select menu.

**Issue**: The current `select_all_item` in `macos.rs` line 139 already carries `Some("Cmd+A")`. Moving it to a different parent submenu doesn't change its accelerator. Good — just confirm this is the intent. Same for `Deselect all` (`Cmd+Shift+A`).

But: when you move a menu item OUT of Edit, on macOS, the conventional Edit menu loses the "Select all" entry, which Cocoa apps virtually always have. macOS users hit ⌘A everywhere expecting Edit > Select all. Moving it is a UX choice. Probably fine for Cmdr (file-list-centric app, "Select all files" is more discoverable in a Select menu), but call it out as a deliberate UX decision in the Decision log so the implementing agent doesn't second-guess and "fix" it back.

**Fix**: Add a Decision log entry: "Select all / Deselect all live in the new Select menu, not Edit. macOS convention puts them in Edit, but Cmdr's `selectAll` operates on files (not text), so the Select menu is the more honest home. Edit retains Copy/Paste/Cut for text operations."

### G6: `selection-history.json` lacks an `excludeSystemDirs` / `scope` field; recent-selection chip click apply path needs spec

**Where**: § History store (lines 200-228); M7 § "Tests".

**Issue**: `applyHistoryEntry` in `search-state.svelte.ts` reads `entry.scope`, `entry.excludeSystemDirs`, `entry.caseSensitive`, `entry.filters`. The factory's `applyHistoryEntry` will need a variant that doesn't touch scope/excludeSystemDirs for Selection. Or the SelectionHistoryEntry shape is different (it is, per § History store). But there's no defined `applySelectionHistoryEntry` and no test for it.

**Fix**: Spec the Selection apply flow explicitly: clicking a recent-selection chip should restore `query` + `mode` + `caseSensitive` + filters. NO scope, NO excludeSystemDirs. Add a test: "click a recent-selection chip → state matches the entry." Specify this in M7's tests.

### G7: AI prompt + folder sample size — IPC payload size

**Where**: § AI translation (line 195): "Return at most 240 names." § R1.

**Issue**: 240 filenames, average ~40 chars each = ~10 KB. Tauri IPC handles that fine. But the prompt rendering also embeds them; total prompt size on a deep folder with 240 long names is ~30-50 KB. OpenAI's API accepts it. Local llama-server's context window doesn't (4K-8K tokens). The plan doesn't disambiguate: does Selection's AI mode work only when AI is set to cloud, or also for local? If local: 240 names at average ~10 tokens each = 2400+ tokens just for the sample, before the prompt and response. Local models would choke.

**Fix**: Decide whether Selection's AI mode requires a cloud provider. Either:
- (a) Cloud-only for now: gate the AI chip visibility on `ai.provider !== 'local'`. Add to M7's tests.
- (b) Local-compatible: cut the sample to ~50 names when provider is local. Add to `folder-sampler.ts` and document.

Recommend (a) for v1. Add to Decision log.

### G8: M7's E2E spec "must run in <1 s"

**Where**: M7 § Tests (line 814).

**Issue**: The dialog open + AI translation round-trip + apply path can't reasonably hit <1 s if AI translation runs against the real OpenAI API (cloud round-trip is 1-5 s). The E2E spec should pin the non-AI happy path (open, type `*.txt`, Enter, assert selection); the AI path is integration-tested separately or covered by manual MCP smoke per M11.

**Fix**: Specify "the Playwright spec covers the Filename mode happy path only. AI is exercised in M11's manual MCP smoke, not in CI." Add an explicit note: "Don't add an AI E2E spec that hits the real API; flakes will kill CI." Mock the IPC in the spec if Filename-only isn't enough.

### G9: Missing test surfaces

**Where**: Across milestones.

Per `docs/testing.md` § "When you add X, also add Y":

- **Missing**: IPC contract tests for the new commands (M5). Six new IPCs, several destructive (`remove_recent_selection`, `clear_recent_selections`), several with multiple positional args. Per the table: "(b) IPC contract test in `lib/ipc/*.test.ts` IF the command is destructive, cross-window, or has > 2 positional args." `translate_selection_query(prompt, sample_names)` is 2 args (OK). `clear_recent_selections()` is destructive (yes, contract test). `apply_recent_selections_max_count(maxCount)` is cross-cutting (likely yes).
- **Missing**: a state-machine unit test for the `lastDialogEvent` sequencer once it lives in the factory.
- **Missing**: a proptest on `selection-matching.ts::matchEntries` (it's a pure parser + matcher; proptest "no panics, returns valid indices" is cheap insurance).
- **Missing**: a Rust integration test for the Selection AI translation pipeline with the prompt parser. M5 mentions a real-LLM eval behind a feature flag; also need an offline test with a fake LLM response (mirroring `search/ai/parser.rs`'s table-driven tests).

**Fix**: Add to M5's Tests: "IPC contract tests via `installIpcMock()` for `clear_recent_selections` and `apply_recent_selections_max_count`." Add to M7: "proptest on `matchEntries` (returns ≤ totalCount indices, no duplicates, all in [0, totalCount))." Add to M5's offline tests.

### G10: `lib/search/CLAUDE.md` is 703 lines of decisions/gotchas; the split into `lib/query-ui/CLAUDE.md` needs an explicit decision tree

**Where**: M3 § Docs (line 580), M4 § Docs (line 651), M10 (line 935).

**Issue**: The 703-line `lib/search/CLAUDE.md` mixes: (a) shared-orchestrator decisions (Enter ownership swap, IME guard, two-cursor hover, auto-apply gates, dialog overlay vs ModalDialog, capture-phase Esc, recent-search popover reusing FilterChipPopover); (b) Search-specific decisions (snapshot store + virtual volume, MCP open path, index lifecycle, scope row, excludeSystemDirs, Open-in-pane handoff, the AI label vs prompt, cross-snapshot delete sync, capability flags, source-side-ops-from-snapshot, search-results-pane navigation, `nested-interactive` a11y exception, R3 polish items B1-T1).

The plan says (a) moves to `query-ui/CLAUDE.md` and (b) stays in `search/CLAUDE.md`. Sound principle, but without an explicit decision tree, two agents producing the two files will duplicate or drop decisions. M10 ("sweep all CLAUDE.mds") is too late — the agent has to reconstruct intent from a tangled source.

**Fix**: Before M3 starts, write a one-page split sheet inside the plan: list every load-bearing decision and gotcha currently in `lib/search/CLAUDE.md` and tag each one as `query-ui` / `search` / `both` (with cross-link). The implementing agent moves verbatim with a tag. M10's sweep then verifies, not reconstructs.

### G11: M11 includes `desktop-e2e-linux` slow check, but Selection's `=` / `-` keystroke handling is platform-specific

**Where**: M11 § "What" (line 953); M8 § Linux fallback note (line 875).

**Issue**: Linux's keyboard handling for `=` and `-` may differ (non-Latin layouts, AltGr layers). The plan doesn't specify a Linux-specific test. The Linux E2E suite runs in Docker (`apps/desktop/test/e2e-linux/`), which has a known limited keyboard model.

**Fix**: Add a sub-bullet to M7's tests: "Add an e2e-linux test that opens the Selection dialog via `dispatchMenuCommand('selection.selectFiles')` and confirms the dialog renders (key-press path covered separately on macOS Playwright)." That avoids the keyboard-layout pitfall in Linux Docker.

### G12: `RecentItemsFooter` rename and adapter callback shape

**Where**: M3 § "What" (line 567): "Wire props for an entry adapter `(entry: HistoryEntry) → { label, tooltip, mode, age }`".

**Issue**: The current `RecentSearchesFooter.svelte` reads `HistoryEntry` from `$lib/tauri-commands` directly (which is a Search type — `SearchHistoryEntry` essentially). Selection's `SelectionHistoryEntry` is a different type. The plan introduces an adapter, but `HistoryEntry`'s `mode` field is `SearchMode = 'ai' | 'filename' | 'regex'`, and Selection's modes might be the same union or might exclude `'content'` (which Search also excludes). Confirm.

Also: `recent-chips-layout.ts` packs chips by measured width; the adapter must produce a `label` that fits the packing. Existing Search labels are `query` truncated; Selection labels are the same shape. Should work, but pin in M3's tests.

**Fix**: Specify in M3: "the `RecentItemsFooter` `entry` prop type is generic; the consumer (Search or Selection) instantiates with its own entry type and provides the `adapter` callback. Adapter returns `{ label, tooltip, mode, ageLabel, ariaLabel }`. Updated tests cover both consumer wrappings." Add a test for the Selection wrapping in M7.

### G13: Decision log entry for tab order on the title bar

**Where**: § Title bar.

**Issue**: The new title bar is 32 px, centered, with text + optional close button. Where does it sit in the dialog's Tab order? If the close button is `tabindex=0`, it intercepts Tab from the bar before the user reaches the mode chips. If `tabindex=-1`, it's not keyboard-reachable.

**Fix**: Spec it: "Title bar text is non-interactive. Close button, if rendered, is `tabindex=-1` because Escape already closes. The title bar is not in the Tab order." Or: "no close button; Escape is the only close path." Either way, document and test.

### G14: `runHintCopy: string` mentioned only in "Open questions"

**Where**: Open questions (line 1052): "change to 'Press Enter to filter' only when the dialog is in Selection mode; pass via the consumer's config (`runHintCopy: string`)."

**Issue**: This is a real config field. Either it's in `QueryDialogConfig` (M4) or it's not. "Open questions deferred to execution" doesn't bind it.

**Fix**: Lock it into `QueryDialogConfig` in M4. Default `"Press Enter to search"` for Search. Selection passes `"Press Enter to filter"`. Decided.

### G15: `aiContext` callback timing

**Where**: § QueryDialogConfig (line 628).

**Issue**: `aiContext: () => string[]` — when does it get called? On every keystroke? Once per AI translation? If on every keystroke, sampling 240 names from the focused folder's listing repeatedly is wasteful (and the focused folder can change while the dialog is open if the user dispatched it from a different pane than what's now active). If once per AI translation, the sample is stale by the time it lands in the prompt.

**Fix**: Spec the call timing: "`aiContext` is called once per AI translation, immediately before the IPC call. The Selection dialog snapshots the focused pane's listing at dialog open; if the focused pane changes mid-dialog, the snapshot doesn't refresh (the user opens, decides what to select, applies). Spec'd in M7."

### G16: Cross-snapshot delete + Selection interaction

**Where**: Not addressed.

**Issue**: Search's snapshot panes can have entries removed mid-session via the `removeEntryFromAllSnapshots` hook on transfer-complete. If Selection's dialog is open on a snapshot pane and an entry vanishes, the matched indices the dialog computed are now stale.

**Fix**: Add to M7's spec: "Selection's matcher runs at commit time, not at preview time. Apply reads the snapshot's current entries (via `getNameFor`) and computes indices fresh on the closing keystroke. If the snapshot shrunk during the dialog, the matcher operates on the new shape." Add a test (synthetic: open dialog on snapshot, mutate snapshot, commit, confirm matches use new entries).

### G17: Plan's prose style drift — em dashes, passive voice

**Where**: Multiple. For example, line 1018 "—" (em dash); "the executing agent picks final copy" (passive-ish).

**Issue**: Per `docs/style-guide.md`: no em dashes; active voice. The plan itself has a few em dashes (search the file for `—`) and the plan sets the bar for the implementation agents. Code comments and CLAUDE.md updates the agents produce will mirror the plan's style.

**Fix**: Replace em dashes with parens / commas / colons / new sentences. Quick search-and-fix.

(Not a blocker; the implementation isn't affected. But the plan's own writing matters as a style anchor.)

## 🟢 Nice-to-haves

### N1: Naming — `QueryDialog` reads fine, `QueryModeToggleGroup` is a mouthful

`QueryModeToggleGroup` is 21 chars. Existing `SearchModeChips` is 16 and conveys the same idea. If you keep the bespoke implementation (B2 option a), call it `ModeChips` (in `lib/query-ui/`). If you migrate to the generic toggle (B2 option c), `ModeChips` still works — the component IS the mode-chip row; whether it's built on a `ToggleGroup` primitive underneath is an implementation detail.

Recommend: `QueryDialog`, `QueryBar`, `ModeChips`, `AiPromptStrip`, `FilterChips`, `QueryResults`, `RecentItemsFooter`, `RecentItemsPopover`. Drops 3-5 chars per name with no loss of clarity.

### N2: `lib/query-ui/` directory vs. `lib/query/`

`query-ui` correctly signals "UI components for query dialogs." Keep. (Considered `lib/query/` but that suggests it owns a query model; the UI is what we're sharing.)

### N3: The `runOnMount` flag for Selection

Selection doesn't use `runOnMount` (no MCP entry, no prefill). The plan acknowledges this (line 94: "Search uses it for MCP, Selection ignores it but the hook exists in the shared component"). Fine. Add a default-`false` for Selection and skip the effect in the wrapper.

### N4: `cargo mutants` scope in M10

Plan says: `cargo mutants --file src-tauri/src/selection/history.rs --file src-tauri/src/selection/ai/parser.rs`. Add `src-tauri/src/selection/ai/query_builder.rs` too — it has branchy logic for assembling the result from the parsed LLM response.

### N5: Empty-state copy

Plan's example chips for Selection (line 408): `[ ✨ all image files ]     [ ✨ logs newer than a week ]     [ ✨ files bigger than 5 MB ]` followed by `[ *.pdf ]     [ *report* ]     [ /^\d{4}-/ ]`.

The Filename example `*.pdf` doesn't match any of the AI examples (image files, logs, big files). Pick examples that pair: AI "all image files" → filename `*.{jpg,png,gif}` (or a single glob like `*.png`). AI "logs newer than a week" → filename `*.log`. AI "files bigger than 5 MB" → no filename equivalent (size is filter-chip-only). Mix and match so the user sees the parity.

### N6: Plan's "we will NOT use `ModalDialog`" call (line 254-255)

Repeat in the consumer wrapper docs. Otherwise, future agents wrapping `SelectionDialog` for, say, an in-line variant might try to use `ModalDialog` and get bitten.

### N7: Cargo deny — Selection has no new deps

Plan correctly says none. Confirm in M5: "No new Cargo deps; no `cargo deny` re-run needed."

### N8: The `MAX_HISTORY_PER_TAB = 100` integration

Not affected by Selection (no snapshot promotion). But the parallel design (Selection has its own `selection-history.json` with a cap) should reuse the same "cap == 0 disables" convention as Search. Spec it: `selection.recentSelections.maxCount = 0` disables persistence entirely.

### N9: AI eval feature flag naming

Plan says `#[cfg(feature = "ai-eval")]`. The existing Search AI eval (if there is one) might use a different feature name; check `Cargo.toml`. If a flag exists, reuse it.

### N10: Run cadence

Plan says "FF-merged to main without review per David." Add: "Each milestone runs `./scripts/check.sh` green before merging; commits do not pile up uncommitted across milestones." Cmdr's `--include-slow` cadence (per AGENTS.md) is "before declaring a milestone done." M11 covers this explicitly; sub-milestones (M1-M9) just run the default suite. Confirm M10 also runs `--include-slow` (it should, per AGENTS.md "before declaring feature done").

### N11: M2's "~100 call sites" estimate

It's worth running a quick `rg "import .* from '\$lib/search/search-state'" apps/desktop/src/ | wc -l` to get the actual count before M2. Setting expectations: 50 vs. 150 sites is a 3x difference in churn.

### N12: ToggleGroup test naming

If B2 is resolved by keeping the bespoke chip strip (option a), drop `ToggleGroup.a11y.test.ts` and `ToggleGroup.test.ts` from M1. Saves time.

## Overall verdict

**REVISE.**

The plan's shape is sound: factoring `SearchDialog` into `QueryDialog` + consumer wrappers is the right call, the milestone sequencing is correct, the risk register is thoughtful, and the design hits the elegance bar. But the wrong assumptions in B1 (Ark UI claim) and B2 (toggle-group vs tab-strip semantics), the contradiction in B5 (bare `=` vs Shift+= mismatch), the contradiction in B3 (snapshot-pane accessor vs displayed `name`), and the silent gaps G1 (capabilities), G3 (CommandScope), G4 (platform menu files), G7 (search-specific state in the shared factory) will each send an implementing agent in the wrong direction. The remaining 🟡 issues are mostly tightening: spec a config field, lock a deferred answer, name a test surface.

After a revision pass that:
- corrects the Ark UI claim and picks an explicit primitive (B1 + B2 + N1 + N12),
- locks the keystroke binding (B5) with a unit test contract,
- pins the snapshot-pane accessor (B3) with a test,
- swaps `gpt-5.5` for "configured model" (B4),
- spells out the wrapper's responsibility and drops the line-count target (B6),
- specifies the M2 state factory split (B7 + B8 + G6),
- adds capability + scope + menu-file specifics (G1 + G3 + G4),
- locks the deferred config fields (G14 + G15),
- adds the missing tests (G9),
- handles AI provider gating (G7),
- writes the CLAUDE.md split sheet up front (G10),
- and sweeps em dashes / passive voice (G17),

the plan should be ready for execution. Estimate: 1-2 hours of revision for a focused writer.
