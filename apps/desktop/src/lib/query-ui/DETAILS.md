# Query UI details

Pull-tier docs for `lib/query-ui/`: architecture, flows, and decision rationale. Must-know invariants and gotchas live
in `CLAUDE.md`.

## i18n

All user-facing copy in this area lives in the `queryUi.*` message catalog (`$lib/intl/messages/en/queryUi.json`),
resolved through `tString()` / `t()` (and `<Trans>` for the keyboard-tip, recent-popover-hint, and scope-hint sentences
that interleave key-cap chips / `<code>` glyphs). `cmdr/no-raw-user-facing-string` is enforced on `/lib/query-ui/`, so
add new copy as a catalog key, not a literal. Pure helpers that compose chip/tooltip strings (`filter-chip-state.ts`,
`filter-popover-helpers.ts`, `recent-items/recent-items-utils.ts`, `ai-summary.ts`) call `tString()` directly; counts
are passed in as preformatted `*Text` params (with a raw integer alongside only to drive plural selection). Symbol-like
mode badges (`AI` / `.*` / `Aa` in `modeBadge`), unit abbreviations (`kB` / `KB` / `MB` / `GB`), and the regex
slash-wrap are typography, not copy, and stay literal. Parity net: `queryui-i18n-parity.test.ts`.

**Consumer-supplied copy overrides the `queryUi.*` default, and the fallback lives at the call site.** A few strings
belong to the DIALOG rather than to the shared primitive, because the two consumers name the same control with different
verbs: `config.runHintCopy` (Search "Press Enter to search" / Selection "Press Enter to filter") and
`config.recentItems.triggerAriaLabel` / `.triggerTooltip` / `.filterPlaceholder` / `.emptyMessage` / `.popoverAriaLabel`
/ `.listboxAriaLabel` (recent searches vs recent selections). Each is optional and resolved as
`config.X ?? tString('queryUi.…')` where it's passed down, so a new consumer gets a sensible shared default and an
existing one keeps its own voice. ❌ Don't re-hardcode a `tString('queryUi.…')` inside the child component when the
config already carries the string: that silently ignores what the consumer asked for, which is how the Selection dialog
spent its life telling users to "Press Enter to search".

Home for primitives shared between the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`).
Owns the unified query bar, mode chips, AI prompt strip, filter chips strip (size, modified, scope, pattern),
virtualized results table with path pills and per-row menus, the query field's recent-items dropdown, and the
cross-consumer filter state factory.

See `../search/CLAUDE.md` for Search-specific decisions (snapshot store, virtual volume, MCP open path, "Open in pane",
index lifecycle, "Use current folder" smart fallback) and `../selection-dialog/CLAUDE.md` for Selection-specific
decisions (matcher in JS, cloud-only AI, commit-on-Enter, snapshot-pane banner).

Filter-chip internals (chip strip, single chips, popover anatomy, the chip-popover focus contract, grid-style Size /
Modified popovers, shortcut openers, and chip-specific decisions) live in `filter-chips/CLAUDE.md`.

## QueryDialog orchestrator

`QueryDialog.svelte` is the shared overlay every consumer mounts. It owns the overlay chrome, the keyboard contract, IME
guard, auto-apply gates, the `⏎` ownership swap, the `lastDialogEvent` lifecycle, the title bar, the chip strip, the AI
prompt strip, the results table, the recent-items dropdown, the empty state, and the optional notice banner. Consumers
wire everything Search-or-Selection-specific through a single [`QueryDialogConfig`](query-dialog-config.ts) prop.

The config carries the title + max width (+ an optional stability `badge` rendered as a `StatusBadge` next to the title;
both consumers derive it from `getBadgeStatus()` in `$lib/feature-status`), the cross-consumer state instance (the
factory output), an `aiEnabled` flag, the per-chip visibility set, a `showPathColumn` flag, the run-hint copy, the
history store + adapter + key, the empty-state hints, the filter-chips extras, the index lifecycle flags, an optional
`noticeBanner`, the async `runQuery` + optional `translateAi` callbacks, primary + secondary action descriptors,
callbacks for path-pill / example / row-menu / recent-activate / recent-remove / close events, optional `onMount` /
`onDestroy` / `onClearState` hooks, and two optional consumer-owned snippets bracketing the results table, each owning
its own data + lifecycle: `resultsNotice` above it (a caveat about the answer — Search's coverage note) and
`resultsExtra` below it (a second result kind — Search's "text in images" OCR grid). Other consumers omit both.

### The controller split

`QueryDialog.svelte` holds the wiring and the layout; four siblings hold the behavior, each with its own unit tests. The
component keeps only what genuinely needs the component: the `$derived` readers off `config.state`, the two `$effect`s,
the mount/destroy hooks, the DOM refs, and the template.

- **`query-runner.svelte.ts`** — everything between "the user wants results" and "state holds them": the nothing-to-run
  guard, the auto-apply debounce and its gates, the IME guard, `executeQuery`, the `runAiSearch` round-trip, the
  streaming run (§ Streaming), and the `hasSearched` / `highlightedFields` / `live` flags the template renders off. Also
  exports two pure helpers the component's effects call, `hasRunnableQuery(state)` and `shouldShowRunHint(...)`.
- **`query-stream.ts`** — the answers-over-time contract plus its pure pieces: the status sentences, the phase labels,
  and the announcement throttle. No Tauri, no coverage vocabulary; § Streaming has the whole shape.
- **`recent-popover.svelte.ts`** — the dropdown's open flag plus its focus-restore rules (`close()` defers a frame and
  yields to whoever claimed focus; `closeAndFocus()` waits a tick past the popover's own focus trap).
- **`query-shortcuts.ts`** — pure key routing: `matchKey` (modifier-superset rejection + the macOS Option-glyph `code`
  fallback), `modeForShortcutNumber`, and `routeModifierShortcut(e, handlers)`.
- **`result-actions.ts`** — what ⏎ / ⌥⏎ / a row click / the footer buttons do with the current result set, including the
  Selection-style "no secondary action" fallback.

`createQueryRunner` takes `getConfig`, a GETTER, not a config value. Consumers build `config` in a `$derived`, so it's a
fresh object on every reactive change; a captured reference would freeze gates like `isIndexReady` and `inputsDisabled`
at their mount-time values. Same rule for any future controller extracted from here.

The runner's `highlightedFields` is ONE `SvelteSet` mutated in place, never reassigned. `SvelteSet` is reactive per key,
so a reader that called `.has('query')` stays subscribed to the instance it read; swapping in a fresh set leaves every
reader watching the old one and the AI flash silently stops repainting.

### Ownership contracts

Three pieces of state are QueryDialog's alone; the consumer's callbacks MUST NOT write to them:

1. **`state.lastDialogEvent`** is QueryDialog's. The orchestrator writes `'opened'` on mount, `'query-edited'` on bar
   input, `'filter-edited'` on FilterChips edits, `'cursor-moved'` on ↑/↓ and hover, and `'results-arrived'` after
   `runQuery` resolves. Writing it from a consumer callback breaks `deriveEnterAction` and the `⏎` ownership swap.
2. **`state.lastAiPrompt` / `state.lastAiCaveat`** are QueryDialog's. The orchestrator sets the prompt to the trimmed
   user input BEFORE invoking `translateAi`, and sets the caveat to whatever the consumer's callback returns. The
   consumer's `translateAi` returns `{ caveat, highlightedFields }` only.
3. **`state.results` / `state.totalCount` / `state.cursorIndex`** are QueryDialog's after `runQuery` resolves. The
   consumer's `runQuery` returns `{ entries, totalCount }` and never touches the state.

The split keeps the `⏎` ownership swap deterministic and lets the orchestrator drive the AI strip lifecycle (clear on
the next non-AI run, etc.) without each consumer re-implementing the rule.

### AI translation errors surface here, once, for both consumers

`runAiSearch` invokes `config.translateAi` inside a `try/catch`. The consumer's `translateAi` does NOT swallow the IPC
error — it lets the typed `AiTranslateError` throw. QueryDialog catches it and calls `showAiTranslateErrorToast(err)`
(`$lib/ai/translate-error-toast`), which maps the error's `kind` to a specific, friendly toast (out of quota, key
rejected, timed out, empty answer, …). Both Search and Selection get the same error UX from this one place; don't re-add
a per-consumer `catch` that returns `null` silently. A consumer's `translateAi` returning `null` still means a benign
empty translation (nothing to apply, no toast) — distinct from a throw.

### Chrome: it's a `ModalDialog`

Every bit of dialog chrome comes from `$lib/ui/ModalDialog`: the 27 px `--radius-dialog` corner, the two-hairline macOS
panel edge, `--shadow-dialog`, the standard `<h2>` title bar with its × button and drag handle, `use:trapFocus`, the MCP
`notifyDialogOpened` / `notifyDialogClosed` pair, the close registry entry, and focus restore on unmount. QueryDialog
must NOT re-implement any of them; `config.dialogType` is already a `SoftDialogId`, so it maps straight onto `dialogId`.
The title snippet renders `config.title` in its own `<span>` (the optional `StatusBadge` is a sibling), so a consumer
test can address the words alone.

Five opt-ins carry the shape this dialog needs (each documented in `$lib/ui/DETAILS.md` § ModalDialog):

- `align="top"` — the Spotlight placement, 10vh from the top.
- `fillBody` + `containerStyle="… max-height: 80vh"` — a fixed-height frame whose body is a flex column, so
  `.results-well` (the only `flex: 1 1 auto` descendant) absorbs the slack while every strip keeps its intrinsic height;
  `.results-container` inside the well is what actually scrolls.
- `ownsKeyboard` — `handleKeyDown` owns the whole contract: Enter (the `⏎` ownership swap) and the capture-phase Escape
  that defers to an open `.ui-popover`. `ModalDialog` still `stopPropagation()`s (shielding the explorer behind the
  scrim) and still drives `onclose` from the × button, the focus-trap escape fallback, and the MCP close registry.
- `closeOnOverlayClick` — a scrim click dismisses, as it always has here.
- `overlayClass="search-overlay"` — the one stable structural hook across all three `dialogType`s. The E2E suite
  (`test/e2e-playwright/search-helpers.ts`, `i18n-capture-surfaces.ts`) and the overlay-dismissal safety net key on it;
  `data-dialog-id` can't name "the query dialog" because there are three of them.

### Zones

The body reads as three zones. Only ONE of them paints a surface: everything above the results sits on the dialog
panel's own `--color-bg-dialog`, and the separation comes from spacing plus single hairlines.

1. **What to look for and how** — the `.query-grid`, then the `AiPromptStrip` / notice banner when present.
2. **The filters** — `FilterChips`. No band fill; one bottom hairline, which is the seam into the results.
3. **The results** — `QueryResults` (header + rows + states + status bar) on `--color-bg-primary`, a recessed well
   against the panel. That flip is what separates "how do I narrow this" from "here's what I found"; the footer's top
   hairline closes the well from below.

Don't reintroduce per-strip backgrounds. Three surfaces in one panel is the look this replaced, and the well only reads
as a well while it's the only surface flip in the dialog.

**The `.query-grid` is a real 2×2** (`minmax(0, 1fr) auto`): query field over mode chips on the left, Search button over
the Count-only switch on the right. `QueryBar` is `display: contents`, so its two halves land directly in row 1's cells;
`.query-bar` survives only as the selector hook the E2E suite and the dialog tests address the field and the run button
through. The single `auto` right column is shared by both rows, which is what makes the pairs line up, and the run
button stretches to fill it. With no Count-only switch (Selection), the chips span both columns instead of leaving a
hole: four mode cells don't fit a narrowed left column at the dialog's minimum width (`min(720px, 60vw)` against a 950px
window).

The Count-only switch is a QueryDialog-level sibling of `ModeChips`, not a `ModeChips` or `FilterChips` child: it
changes what the search RETURNS rather than what it matches, and `ModeChips` is a pure `ToggleGroup` wrapper. It's the
house `$lib/ui/Switch`; a hand-rolled `role="switch"` is rejected by `cmdr/prefer-ui-primitive`. Flipping it re-runs via
`scheduleSearch()` (debounced, and a no-op in AI mode, which keeps the explicit-trigger contract).

Every strip sits at `ModalDialog`'s `--spacing-dialog` (20 px) side inset, the same as its title bar, so the title, the
query field, the chips, and the footer actions all share one left edge — and they get it for free: a strip sets VERTICAL
padding only, and re-adding a horizontal `--spacing-dialog` would double the inset. The results zone is one step further
in: `.results-well` takes the body inset like any other block, rounds its corners, and clips the header, list, and
status bar inside it, and those three pay `--spacing-md` of their own so the rows breathe inside the well rather than
starting on its edge. The centered state blocks inside `QueryResults` (loading / no-results / empty) are content
padding, not strip inset, and stay on the generic scale.

### Recent-items dropdown

The query field is a combobox over the recent-items history. The field itself is the house `TextInput`
(`radius="full"` + the magnifier `leadingIcon`, the same search pill as the Settings sidebar) with a chevron in its
trailing slot; the list is `RecentItemsPopover` anchored to the pill frame, so the dropdown lines up under the whole
field rather than under the chevron.

**Why not the house `Combobox`** (or Ark's directly): Ark's model uses the control's own input AS the filter. This
design needs TWO independent text fields — the query the user is composing and the dropdown's own fuzzy filter — which
fights Ark's focus routing and would put a focusable input among `role="option"` children. So the pieces are assembled
here instead, and `Popover` keeps supplying the `.ui-popover` Escape-deference contract that QueryDialog's capture-phase
Escape depends on.

**Openers**: the chevron, `⌘H` (toggles), and `ArrowDown` in the field. `ArrowDown` opens ONLY when there are no results
to walk: with a result list, `↓` keeps moving the cursor, which is the more valuable use of the key. So the gesture
lands on a key that was otherwise dead (`handleArrowNav` returned early on an empty list), and it never steals result
navigation. The chevron and `⌘H` work regardless.

**Keyboard contract inside the dropdown** (`RecentItemsPopover.handleKeydown`):

- `↑` / `↓` move the cursor and NEITHER wraps. Wrapping past the end while the user holds a key reads as teleporting.
- `↑` on the TOP row EXITS: it fires `onExitTop`, which closes the dropdown and returns focus to the query field with
  its text untouched (nothing was picked, so there's nothing to undo).
- `Enter` (and a click) SELECTS: the entry loads into the dialog and the dropdown closes. It does not run. QueryDialog's
  `pickRecent` then sets `lastDialogEvent = 'query-edited'`, so `⏎` owns "run-search" and the next Enter runs it.
- Everything else is left alone, so `⌘C` / `⌘V` / `⌘X`, `←` / `→` (with or without modifiers), and `Home` / `End` behave
  like they do in any text field. Typing goes to the dropdown's filter field because `Popover` focuses the first
  focusable child on open.
- **Every key the dropdown claims also `stopPropagation()`s.** The popover renders inside the dialog, which has its own
  `onkeydown`; without this, `↑` / `↓` also move the results cursor underneath and `Enter` also fires the dialog's Enter
  handler. Pinned by `RecentItemsPopover.svelte.test.ts` against a stand-in host listener.

**Focus return.** `Popover`'s Escape path calls `onClose()` then `anchor.focus()`, and the anchor is the pill `<div>`,
which isn't focusable. `closeRecentPopover` therefore re-focuses the query input on the next frame, but only when
nothing else has claimed focus by then — otherwise a click-outside would yank focus off whatever the user clicked. The
keyboard paths (Enter-select, `↑`-at-top, `⌘H`) use `closeRecentPopoverAndFocus`, which focuses after a `tick()` so the
still-mounted popover's focus trap can't pull it back.

**Row content.** Each row is a mode badge, the label with fuzzy-match highlighting, and a quiet meta line: the age, then
`metaLabel` (result count, then the filter summary). Both come pre-formatted from the consumer's adapter (`formatAge` /
`rowMeta` in `recent-items-utils.ts`), so the component never reads an entry field. The full picture (mode name, every
filter, the count) stays in the row's `title` tooltip — the meta line is for recognition at a glance, not for reading.

### Streaming: a query whose answer arrives over time

Search of ground the index doesn't cover walks it, so its answer lands in batches over seconds or minutes instead of one
promise. Selection matches a pane listing it already holds and never streams, which is why the whole mechanism is
OPTIONAL and lives behind `config.streamingSource`.

**The split.** The consumer owns the wire (Search's `lib/search/live-search-source.ts` wraps the four Tauri events); the
RUNNER owns the run. `query-stream.ts` is the vocabulary between them, and it is deliberately free of Tauri, coverage,
and walks: a `QueryStreamProgress` (phase, batch, counts, `capped`), a `QueryStreamEnd` (`matchCount`, `incomplete`,
`walked`, `capped`), and a `QueryStreamSource` (`start` → teardown, `cancel`, optional `rankOnCompletion`). That's what
keeps Search's vocabulary out of the shared dialog while the state writes stay where the ownership contract puts them.

**The phase is RENDERED, ❌ never derived.** `QueryStreamPhase` has one value per thing a run can be doing, and each
maps to one sentence (`livePhaseLabel`) — so a new phase means a new branch and a new catalog key, ❌ never a fallback
onto a neighbouring phase's sentence. Two rules ride on getting that right: `liveWalkProgress` reports folders scanned
for `walking` ALONE (any other phase showing "0 folders scanned" reads as a stuck walk), and a count-only run holds its
"0 so far" back through every phase where it has counted nothing yet. The phases themselves, and which of the run's
states produce them, are the backend's: `src-tauri/src/search/live/DETAILS.md` § What a run says it's doing.

**Progress is a count and a path, ❌ never a percentage or an ETA.** How much a walk has left is unknown by definition
(the frontier is exactly the ground nothing has measured), so a bar or a remaining time would be invented, and honest
progress is a product principle (`docs/design-principles.md`). Directories scanned plus the folder under the walker is
everything the run actually knows.

**The generation guard is the load-bearing one.** The runner mints the run id and hands it to `start`, so no update can
arrive against an id it hasn't seen. Superseding a run does NOT cancel the work behind it (Decision 11), so the previous
run's batches keep arriving; every callback checks the id first and drops anything else. Without it, a refined query
splices its predecessor's rows into the list with no error and no warning. `query-runner.streaming.test.ts` pins it.

**Cancel versus supersede.** A new run drops the old subscription (`dropLiveSubscription`) and lets its work carry on.
`cancelLive()` calls the source's `cancel` and then waits: the end state is the RUN's own word (a terminal update
relabels it), never an optimistic local flip. `dispose()` also only unsubscribes — whether the work outlives the dialog
is the consumer's call (Search's `releaseSearchIndex` stops every live run but the one a pane is being fed by).

**`resume` adopts a run that outlived the last dialog.** The runner calls it once, on mount, BEFORE the
reopen-with-results decision, and a run handed back wins: re-running would SUPERSEDE the live one and strand whatever it
was still feeding (Search's "Open in pane" leaves a walk filling that pane). It's synchronous by design — the decision
gates the mount path, and the consumer is already listening, so this only adds a second reader. The resumption carries
`missedEntries`, the rows found while nobody here was looking; without them the dialog shows a count its list can't
account for.

**Rows, cursor, order.** Batches append in arrival order; the cursor is held by PATH identity, so it stays on its row
across every batch and across the completion re-rank. `lastDialogEvent` flips to `'results-arrived'` on the FIRST batch
only — every later one would overwrite the `'cursor-moved'` the user just caused, which is also the flag that suppresses
the re-rank. The re-rank runs once, on completion, only when `walked` (an index-only answer arrived ranked already) and
only when the user hasn't moved the cursor.

**Auto-apply never streams** (Decision 7): the debounce passes `fromAutoApply`, which routes to the one-shot `runQuery`.
Six keystrokes would otherwise start and abandon five multi-minute walks. The consequence worth knowing: the one-shot
path is now the ONLY one that can report a drive with no index at all, so Search's uncovered-drive note and its indexing
offer belong to auto-applied runs, and the note ends with "Press Enter and Cmdr will look through it now."

**What `QueryResults` does differently under a live run.** `isSearching` is true for the run's WHOLE life, so the
pre-streaming rule (spinner replaces the list) would hide every row a walk finds. The gates are now
`!isSearching || streaming` plus `liveWaiting` (a live run with nothing to render yet), which is the only state that
still owns the content area. The status bar becomes the progress strip: the count, the walk's own progress, where it has
got to, and the Stop button.

**The `aria-live` region is an inner span.** A run emits a batch every 100 ms and the visible counter moves with it;
announcing each one floods a screen reader, and an axe audit sees a valid live region and says nothing. So the region
carries a THROTTLED copy (`createAnnouncementThrottle`: one every two seconds, plus every final update, plus never the
same sentence twice). ❌ Don't put `aria-live` back on `.status-bar` itself.

**Escape means two things.** `resolveEscape()` runs popover → stop the run → close, and both the window-capture handler
and the dialog's own keydown go through it. Closing on the first press would stop the walk anyway (teardown cancels it)
but would also take the results already on screen away, which is the opposite of what somebody pressing Escape at 40,000
folders wants.

**"Stop the run" is answered once per run, and that's what keeps Escape from trapping the dialog.** `running` clears
only on the run's own terminal event (the local state never flips optimistically, so the label stays the backend's
word), so `cancelLive()` tracks whether this run has already been asked to stop and answers `false` from then on.
Without it, a run whose terminal event never arrives answers "there was one to stop" forever, and Escape can never reach
the close: the dialog is un-closable by keyboard until the window reloads. `cancelSearch` doesn't rescue it either,
since a run the backend never registered is one it has nothing to cancel. One such run in the Playwright suite left the
search dialog open for the remaining 44 tests on its shard.

### Count-only results

Count-only is Search-only (`config.filterChipsExtras.countOnly` / `onToggleCountOnly`; Selection leaves both undefined
and neither the switch nor the button renders). The backend returns a total and NO rows, so `QueryResults` replaces the
list with one body-size sentence carrying the thousands-separated total in bold (`queryUi.results.countOnly.sentence`, a
`<Trans>` message whose `<total>` tag lets each locale place the number), plus a "Show results" `Button`.

**The trap**: turning count-only off does not repopulate the list, because there were never any rows to show. So
`showResultsFromCount()` flips the flag AND re-runs, through `runFromButton()` — the same path the bar's `⏎` button
takes. NOT `scheduleSearch()`, which no-ops in AI mode and whenever `search.autoApply` is off; a naive wiring leaves the
user staring at a stale count.

The column header renders only when result rows do (the `showingRows` derived). Column labels over a spinner, a criteria
list, the empty state, or a bare total describe a table that isn't there, and they're the loudest thing in an otherwise
quiet area. The seam is the chip strip's own bottom hairline plus the zone-2 → zone-3 surface flip.

### Nothing to run is not a run

`hasRunnableQuery()` is the single predicate for "is there anything to ask?": a non-empty trimmed query, OR size ≠
`any`, OR date ≠ `any`, OR type ≠ `both`. It gates `executeQuery()` (the choke point every path funnels through:
auto-apply, the ⏎ button, bare Enter, the `runOnMount` prefill) and short-circuits `scheduleSearch()` before its other
gates. When it's false, `resetToEmptyState()` drops `results` / `totalCount` / `cursorIndex` / `lastRunQuery` / the AI
strip and clears `hasSearched`, so the results area falls back to the empty state instead of leaving the previous run's
rows on screen implying they still match.

Why the guard exists: the backend refuses a filter-less, pattern-less run with "Query too broad", and `executeQuery`'s
catch toasts that message — so simply clearing the query field produced a warning toast for a query the user never asked
for.

**An empty pattern WITH an active filter stays runnable.** `≥ 1 MB` with no glob selects every file ≥ 1 MB; Selection
encodes the same rule in `hasActiveFilter()` + `buildMatchQuery` (`lib/selection-dialog/CLAUDE.md`). Don't widen the
guard to "empty query" — that breaks filter-only queries in both dialogs.

### Run failures surface, they don't vanish

`executeQuery`'s catch toasts `queryUi.dialog.runQueryToast` wrapping the reason. The backend refuses some runs with an
actionable message ("Query too broad. Add a filename pattern, size, date, or type filter"); the old bare `catch {}`
turned those into an empty list that read as "nothing matched". No typed variant crosses this IPC boundary, so the
message passes through verbatim rather than being classified by its text; if a typed error kind ever lands, switch on it
the way the AI path switches on `AiTranslateError.kind`. Both consumers get this from the one place, same rule as the AI
toast.

### Lifecycle hooks

- **`onMount`**: invoked once after the orchestrator has wired its own listeners (Esc capture, autoApply setting
  subscription). Search wires `prepareSearchIndex` and the `search-index-ready` listener here; Selection's wrapper
  snapshots the focused pane's listing here.
- **`onDestroy`**: invoked at unmount, before the orchestrator tears down its own listeners. Search wires
  `releaseSearchIndex` here.
- **`onClearState`**: invoked by ⌘N. Consumers wire their full-reset path here (Search's facade clears core + extras
  together; Selection can omit and inherit the core's `clearCore`). The orchestrator also resets its own `lastRunQuery`
  and `hasSearched` flags after the consumer's hook runs.

### `runOnMount` consumer

The orchestrator's `$effect` block on `state.getRunOnMount()` consumes the one-shot prefill flag. It clears the flag
BEFORE dispatching so downstream state writes can't re-trigger the effect. Cold-open (dialog mounts with the flag
pre-set, e.g. MCP `open_search_dialog`) and hot-prefill (the flag flips while the dialog is already open, e.g. a
recent-search activation) flow through the same path. The effect dispatches when there's anything runnable, via the
shared `hasRunnableQuery()` predicate (non-empty query OR size/date/type filter active). AI mode honors the
explicit-trigger contract because the prefill caller's `autoRun: true` IS the explicit trigger.

⚠️ The flag has THREE producers and is consumed once per arming, so a producer that arms it after another one's run has
already fired runs the same query twice. Search's prefill closes that by clearing `lastRunQuery` (a prefill replaces the
session), which is what the reopen producer below reads; see `lib/search/DETAILS.md` § MCP `open_search_dialog`.

A third producer of `runOnMount` is the reopen path. `onMount` sets the flag when the surviving state holds a restorable
NON-AI session (`getLastRunQuery() !== null` AND `hasRunnableQuery()` AND `mode !== 'ai'`), so the dialog re-derives
results on reopen instead of resting on the empty state: Select re-runs the matcher against the freshly-snapshotted
current folder (more correct than rendering rows from the old folder), Search re-hits the index. AI restored sessions
are excluded from this gate (cloud cost); they render the persisted results because `hasSearched` is seeded from
`getLastRunQuery() !== null` at component init. For Search the index may not be ready when `onMount` fires; the effect's
`config.isIndexReady` guard skips the run, and Search's own `search-index-ready` listener re-sets `runOnMount` once the
index loads, so the re-run still lands.

### Test coverage

`QueryDialog.svelte.test.ts` (orchestrator) pins the title rendering, primary + secondary action callbacks, ⌘N / ⌘H, the
IME guard, and the `lastDialogEvent` ownership. The `QueryDialog` block of `dialog.a11y.test.ts` runs axe-core across
loading / index-ready / AI-on against a minimal Search-shaped config. Search's full integration tests live in the
`lib/search/SearchDialog.<concern>.svelte.test.ts` family (session, shortcuts, ai, auto-apply, open-in-pane, scope,
images, coverage, handoff) plus the `SearchDialog` block of `lib/search/search.a11y.test.ts`, and they mount QueryDialog
through the Search wrapper.

The four controller modules carry their own suites (`query-runner.test.ts`, `recent-popover.test.ts`,
`query-shortcuts.test.ts`, `result-actions.test.ts`), which is where a rule is cheapest to pin: the nothing-to-run
guard, the auto-apply gate chain, the spinner clearing on every AI early return, the Option-glyph remap, and the
no-secondary fallbacks all get asserted without mounting a dialog. They build their config from `test-helpers.ts`
(`makeQueryDialogConfig`), the minimal Search-shaped fixture; the mounted-dialog tests keep their own richer one because
they record call transcripts.

## Files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. What the pieces DO is in the sections below: the orchestrator and its ownership contracts in § "QueryDialog
orchestrator", the ToggleGroup wrapper in § "Mode chips", the pill fitting algorithm in § "PathPills measurement", the
state factory in § "State shape contract", the ⏎ swap in § "Keyboard shortcuts", and the orchestrator's test split in §
"Test coverage". `filter-chips/` documents itself (`filter-chips/CLAUDE.md`), and the chip + popover primitives it
builds on live in `$lib/ui/`. Only the layout facts that none of those carry live here:

- **The two consumers render different subsets from the same components.** Search shows all four mode chips; Selection
  drops Content. Search gets the path column (`showPathColumn` defaults `true`); Selection passes `false`. Selection's
  `indexEntryCount === 0` hides `EmptyState`'s "Index ready · …" line. So a change to a shared component has to be
  checked against BOTH dialogs, not just Search.
- **`recent-items/` is generic over the entry type `<E>`.** The dropdown knows nothing about search or selection
  entries: the consumer supplies a `RecentItemAdapter<E>` plus a `keyFn`, and `createRecentItemsState({ getRecent })`
  supplies the store. Reach for the adapter before adding a consumer-specific branch inside the component. The adapter
  also pre-formats the row's meta line (`ageLabel` + `metaLabel`, the latter from `rowMeta()`), so the component never
  reads an entry field.
- **Result rows carry no actions column.** Right-clicking a row (`oncontextmenu` → `onRowMenu`) opens the parent's
  NATIVE context menu, which is the whole of what the old per-row `…` button did (it called the same `onRowMenu`, and
  was `tabindex="-1"`, so no keyboard route was lost). Don't re-add a per-row button: the column ate width the Path
  column needs, and its header label ellipsized to "Ac…" at the dialog's width.
- **`EmptyState`'s example chips come from `config.emptyState.examples`** (forwarded by `QueryResults`), falling back to
  Search-flavoured defaults when a consumer omits them.
- **The results header and the result rows are two separate grid containers, so every track has to resolve identically
  in both.** `ch` tracks resolve against the font-size of the element that owns the grid, so `.column-header` declares
  `--font-size-md` exactly like `.result-row` does; without it the header's `10ch` / `16ch` tracks came out ~14% wider
  (the root size) and the whole right-hand side drifted. The Name track is handed to both as one inline
  `grid-template-columns` string for the same reason. The Path header additionally insets by `--spacing-xxs`, matching
  the horizontal padding on `PathPills`' first pill, so the two left edges line up on TEXT rather than on box edges.

Component tests (`*.svelte.test.ts`) colocate with what they pin; `codegraph_files` lists them. The tier-3 a11y audits
are directory-level, because `svelte-tests` charges per test FILE (`docs/testing.md` § "What a test actually costs"):
`presentational.a11y.test.ts` covers the five components that mock nothing, and `dialog.a11y.test.ts` covers
`QueryDialog` + `QueryResults`, whose `$lib/tauri-commands` / `$lib/settings` / `$lib/icon-cache` stubs would otherwise
apply to the mock-free five too. The one non-colocated suite is `queryui-i18n-parity.test.ts`, the en-locale golden net
described in § i18n: a copy edit lands in the catalog AND in its goldens, together.

## Name column shrink-wrap

`QueryResults`' Name track is a measured pixel width, not the fixed `minmax(80px, 22ch)` it used to be: a list of `test`
files reserved 22 characters and the Path column next to it mid-truncated to crumbs. Same idea as
`file-explorer/views/measure-column-widths.ts`, which shrink-wraps `FullList`'s Ext / Size / Modified.

- The math is pure and unit-tested with mocked widths: `name-column-width.ts` (`computeNameColumnWidth`,
  `visibleRowRange`, and the `22ch` ceiling / `80px` floor / 2 px pad constants). The component owns the DOM reads.
- **Only the rows currently on screen count.** The list isn't virtualized (Search caps at 30 rows, Selection lists one
  folder), so there's no `visible` slice to borrow the way `FullList` does. The range comes from the scroll container's
  `scrollTop` (an `onscroll` handler) and `clientHeight` (a `ResizeObserver`) against the first row's measured height.
  Degenerate geometry falls back to the whole list: a slightly wide column beats clipped names.
- **The measurement cannot oscillate**, and any change here has to keep it that way. Every input — scroll offset,
  viewport height, row height, the row's computed font, and the entry NAMES read from the data — is independent of the
  width being written. Rows are `white-space: nowrap` one-liners, so their height and the container's scroll geometry
  can't move when the track resizes; and we measure `entry.name`, never the DOM text `useShortenMiddle` wrote into the
  cell. The `$effect` reads its dependencies up front and never reads `nameTrack` itself.
- **The measurer is keyed on the row's computed font string**, read off a real `.result-name` cell. A text-size change
  therefore rebuilds it on its own, which is the job `getEffectiveScale()` does on the `FullList` side. It's probed once
  (`candidate('0')`) before adoption, because pretext needs Canvas 2D and only fails on first use; without canvas the
  component stays on the CSS fallback track, identical to the fixed one it replaced.
- **The track eases between widths** (`--transition-slow` on `grid-template-columns`, `prefers-reduced-motion`
  respected), except for the very first measured width, so opening the dialog doesn't animate the column in from the
  ceiling.
- **Selection (`showPathColumn: false`) keeps Name as the `1fr` flex track.** With no Path column there's nothing to
  hand the freed width to, so shrink-wrapping would only open a gap between Name and Size.

## State shape contract

`createQueryFilterState()` owns ONLY cross-consumer fields. Both Search and Selection share the same shape; one dialog's
instance can never leak into the other.

Fields:

- `query`, `mode` (the unified search input + mode discriminator)
- `sizeFilter` + value/unit, plus the `Max` half for `between` ranges
- `dateFilter` + value, plus `dateValueMax` for `between` ranges
- `typeFilter: 'both' | 'file' | 'folder'` (default `'both'`), mapped onto the existing IPC
  `SearchQuery.isDirectory: Option<bool>` — no new IPC field or schema change
- `caseSensitive`
- `lastAiPrompt`, `lastAiCaveat` (the AI transparency strip's content)
- per-mode `handTyped` buffers (`ai` / `filename` / `regex`)
- `results`, `totalCount`, `cursorIndex`, `isSearching`
- `lastDialogEvent` (drives ⏎ ownership via `deriveEnterAction`)
- `runOnMount`, `lastRunQuery` (one-shot prefill + auto-apply gates)

Search-only fields live next to the Search wrapper in `../search/search-extras-state.svelte.ts`: `scope`,
`excludeSystemDirs`, `isIndexReady`, `indexEntryCount`, `isIndexAvailable`, `lastAiLabel`, `lastAiPattern`,
`lastAiPatternKind`. The whole-drive index is Search-only (Selection matches against an in-memory pane listing), so the
index flags live there even though they look like "session state". The Search wrapper instantiates both factories and
composes them; Selection's wrapper uses only the core. This keeps Selection's runtime state free of fields it never
reads, and keeps the shared factory honest about what's actually shared.

### When to use the factory vs extras

When adding a new field, ask: "would Selection also care about this?"

- **Yes** → add it to `createQueryFilterState()`. Cross-consumer. Selection's instance will carry it whether or not the
  Selection wrapper reads it today.
- **No** → add it to `createSearchExtrasState()` in `lib/search/`. Search-only.
- **No, but Selection has its own variant** → keep both in their respective consumer's "extras" module. Don't try to
  share via the core when the semantics diverge.

The `lastAiLabel` field is the textbook "no" case. Search's snapshot pane needs a short LLM-produced title for the
breadcrumb; Selection has no snapshot pane and no breadcrumb to seed.

### `recordAiTranslation` is split

The core's `recordAiTranslation({pattern, kind})` writes ONLY to `handTyped[mode]` — AI's output overwrites the matching
mode's hand-typed buffer. Both consumers call this. The extras' `recordAiPatternAndLabel({pattern, kind, label})` writes
ONLY to the Search-only fields. Search's wrapper calls this right after the core method; Selection's wrapper skips it.
The Search façade in `lib/search/search-state.svelte.ts` keeps a `recordAiTranslation({pattern, kind, label})`
convenience that calls both methods in sequence.

### `switchMode` carries the term into an empty target buffer

Each mode (`ai` / `filename` / `regex`) owns its own `handTyped` buffer. `switchMode(target)` saves the bar's current
text under the outgoing mode's slot, then restores the target's buffer. When the target buffer is **empty**, it seeds
the bar with the **outgoing term** so the user's words follow them across the switch instead of vanishing. A
**non-empty** target buffer is the user's own prior text for that mode and is never overwritten.

This carries across AI↔non-AI too, raw and unconverted: a glob switched into AI lands as a prompt, a prompt switched
into filename lands as a glob. That's a deliberate semantic oddity (the text isn't re-interpreted), accepted because
losing the user's words is worse than handing them text they may need to tweak.

**Precedence on an empty target buffer** (reconciling the carry-over with the AI-pattern probe):

1. `aiPatternProbe(target)` first. It returns the AI's structured, kind-correct pattern (filename gets the glob, regex
   gets the regex) and is the post-AI editing handoff (the "tweak what the agent did" loop depends on it). The raw
   carry-over must NOT clobber it.
2. The outgoing term second, as the fallback when there's no probed pattern.

Selection wires `aiPatternProbe` to `null` (no Pattern chip), so for Selection the carry-over is the only seeder; Search
wires it to its extras module. Pinned by `query-filter-state.test.ts` § "switchMode term carry-over" (both directions,
the non-overwrite guard, and the probe-wins precedence).

## Shared UI behavior

Small contracts that apply to every consumer of the query UI:

- `QueryBar.svelte`'s run button is the house `Button` (`variant="secondary"`, same family as the footer actions) with
  the `⏎` `ShortcutChip` at `--spacing-xs` from the "Search" label, so the rhythm matches "Go to file ⏎" in the footer.
- Each row's `title` tooltip leads with the full text so a CSS-ellipsis-truncated row stays readable on hover.
- Path column font is `--font-size-sm` (matching the filename column) with `--spacing-xxs` row vertical padding so the
  row height stays compact.
- **Fixed interaction keys render as literal `ShortcutChip`s** (`size="sm"` in dense slots): the run button's `⏎`, the
  empty-state tip (`⌘N` / `⌘H` / `⌘Enter`, in `EmptyState.svelte`), the scope popover's `⌥C` / `⌥V`, and the
  recent-items dropdown's `↑↓` / `Enter`. These are dialog-internal keys with no registry command, so the chip only
  unifies their look — never clickable, never dynamic. The mode-chip `.tg-hint` glyphs (`⌥A` / `⌥F` / `⌥R`) and the
  footer action-button hints (`Go to file ⏎`, `Show all in main window ⏎`) deliberately stay un-boxed; see
  `lib/ui/CLAUDE.md` § ShortcutChip for the rationale.

Chip-side behaviors live in `filter-chips/CLAUDE.md`; search-specific ones in `lib/search/CLAUDE.md`.

## PathPills measurement

The fitting algorithm lives in `path-pills-layout.ts::computePathPillsLayout` (pure, deterministic, unit-tested with
mocked widths). The chrome budget per pill is 4 px (matching the real CSS padding) so the strip doesn't collapse when
there's free space. The container width comes from a `ResizeObserver` on the strip element, and `createPretextMeasure`
provides pixel-accurate text widths.

## Keyboard shortcuts (in-dialog, hard-coded)

Both Search and Selection inherit these. ⏎ has dynamic ownership (see D8 below).

- **`Enter`**: Dispatched via `enterAction`: "go-to-file" or "run-search" (D8)
- **`⌥⏎`**: Show all results in the main window (Search) / no-op (Selection); see consumer
- **`⌘Enter`**: No-op. Bare Enter is the only path that runs a search or opens the cursor row.
- **`⇧Enter`**: No-op. Same rule as ⌘Enter.
- **`⌘N`**: Clear all dialog state ("new search" / "new selection")
- **`⌘H`**: Toggle the query field's recent-items dropdown (fuzzy over the full history)
- **`⌘1`**: Switch to AI (AI on) or Filename (AI off)
- **`⌘2`**: Switch to Filename (AI on) or Regex (AI off)
- **`⌘3`**: Switch to Regex (AI on); no-op when AI is off
- **`⌘4`**: Reserved for Content when it ships; not wired now
- **`⌥A`**: Mode chip: AI (global inside the dialog; only when AI is enabled)
- **`⌥F`**: Mode chip: Filename (global)
- **`⌥R`**: Mode chip: Regex (global)
- **`↑` / `↓`**: Move the cursor through the results list (loops top<->bottom). With NO results, `↓` opens the
  recent-items dropdown instead (see § Recent-items dropdown)
- **`←` / `→`**: When focus is on a mode chip: move between chips (skip Content)
- **`Tab`**: Trapped within the dialog (shared `use:trapFocus` on the overlay); cycles through interactive elements
- **`Escape`**: Close the dialog

Filter-popover openers (`⌥S`, `⌥M`, `⌥I`) and the macOS Option-glyph remap live in `filter-chips/CLAUDE.md`.
Scope-popover shortcuts (`⌥C`, `⌥V`) are Search-only — see `lib/search/CLAUDE.md` § "Scope shortcuts".

### `⏎` ownership swap

The factory carries `lastDialogEvent: LastDialogEvent` (one of `opened`, `results-arrived`, `cursor-moved`,
`query-edited`, `filter-edited`). The pure helper `deriveEnterAction({ lastEvent, resultsCount })` returns
`'go-to-file' | 'run-search'`:

- `'go-to-file'` when there are results AND the last event was `results-arrived` or `cursor-moved` (the user just got a
  list back or is browsing it). Pressing ⏎ opens the cursor row in the active pane.
- `'run-search'` otherwise (zero results, freshly opened, query/filter just edited). Pressing ⏎ runs the query.

The bar's run button reads `Search ⏎` only when `enterAction === 'run-search'`; the footer's `Go to file` button reads
`Go to file ⏎` only when `enterAction === 'go-to-file'`. Exactly one of them surfaces the hint at any time. Tests in
`enter-action.test.ts` pin the eight-permutation table.

### Footer buttons always visible

The policy: footer actions render unconditionally; when there are no results (or the index isn't ready) they render
disabled instead of hidden, so the layout stays still while the user types. `QueryDialog.svelte` renders the
primary/secondary footer actions itself, from `config.primaryAction` / `config.secondaryAction`, as standard
`$lib/ui/Button`s (`variant="primary" | "secondary"`, `size="regular"`) with the shortcut hint on a `ShortcutChip`.
Search wires "Show all in main window" (primary, ⌥⏎) + "Go to file" (secondary, ⏎) through that config; Selection wires
"Select these files" (primary, ⏎). There's no separate per-consumer footer component.

The Content chip is visible-disabled with a "Coming soon" tooltip. It has **no** shortcut. Wiring a shortcut to a
disabled control is hostile UX; reserving `⌘4` is the better contract. When Content ships, it claims `⌘3` and Regex
moves to `⌘4`.

## Mode chips: shared visual primitive, two ARIA shapes

`lib/ui/ToggleGroup.svelte` is the shared segmented-control primitive used by both Settings's toggle groups and the
Query dialog's mode chips. See `lib/ui/CLAUDE.md` § "ToggleGroup" for the primitive's contract. `ModeChips.svelte` is
the Query-side wrapper: `semantics="tabs"`, one option entry per mode, the disabled Content entry carries the
`disabled: true, tooltip: "Coming soon: ..."` flags so the chip stays visible-disabled with the tooltip wired through
the underlying ToggleGroup option cells.

Same external props as `SearchModeChips`: `mode`, `aiEnabled`, `disabled`, `onSelect`.

## Key shared patterns

**Command-palette behavior on standard chrome**: the dialog keeps its palette-style keyboard model (arrow keys through
results, Enter with dynamic ownership, popover-aware Escape) while rendering as a `ModalDialog`. `ownsKeyboard` is what
makes that possible; see § Chrome.

**Two-cursor hover model**: `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are independent. Hovering a row writes
`cursorIndex` via `onHover` so mouse + keyboard share one accent-colored cursor.

**Live search with debounce**: 1 s debounce on filename/regex modes only, gated by the `search.autoApply` setting
(default on). AI mode never auto-applies regardless: AI calls cost money and the user must explicitly opt in via Enter /
`⌘Enter` / the `⏎` run button. Constant `SEARCH_AUTO_APPLY_DEBOUNCE_MS = 1000` lives in `query-filter-state.svelte.ts`.

**Auto-apply gates**: `scheduleSearch()` returns early in four cases:

0. `!hasRunnableQuery()`: nothing to ask, and the previous run's rows get dropped (§ Nothing to run is not a run).
1. `mode === 'ai'`: AI never auto-applies.
2. `search.autoApply === false`: the user runs every search explicitly.
3. IME composition is in progress.

**`⏎` run button**: Always visible on the right end of the bar. Clicking it is equivalent to pressing Enter in the
input. Mouse-first path; keyboard-first path is Enter.

**"Press Enter to search" hint**: Appears in the right gutter of the bar in `--color-text-tertiary` when (a) the query
is non-empty and (b) it has changed since the last actually-issued search and (c) auto-apply won't pick it up
(`mode === 'ai'` OR `search.autoApply === false`). Tracked by `lastRunQuery`.

**IME composition guard**: The dialog tracks `imeComposing` via `oncompositionstart` / `oncompositionend` on the search
bar input. While composing, `scheduleSearch()` is a no-op so we don't fire mid-character on Chinese / Japanese / Korean
input. On `compositionend` the dialog calls `scheduleSearch()` once so the user gets exactly one auto-apply fire after
the composed character lands.

**Deferred loading indicator**: The "Loading drive index..." message only appears when the user has triggered a search
while the index is still loading. On initial open, the results area is empty (no loading message) since the user is
still typing their query.

**State preservation across close + reopen**: The factory's `$state` survives dialog unmount. Closing the dialog (Escape
or overlay click) does NOT wipe query, mode, filters, scope, results, or cursor. The only reset path is `⌘N` inside the
dialog, which calls the consumer's clear hook. On reopen the dialog shows those results immediately rather than the
empty state: `hasSearched` is seeded from `getLastRunQuery() !== null`, and a restored non-AI session re-runs on mount
(see the `runOnMount` consumer section) so the rows reflect the folder open now. AI sessions render the persisted
results without re-calling the cloud.

**`⌘N` shortcut**: Hard-coded in the dialog's `handleModifierShortcuts`. Captured before the dialog's global
`stopPropagation` would let it reach the route-level `⌘N` (new tab) handler. The choice of `⌘N` matches the macOS "new
X" idiom.

**`runOnMount` flag**: A one-shot boolean on the core factory. Cleared on `⌘N` (so the shortcut doesn't leave a stale
flag). Set by Search's `applySearchPrefill(prefill)` (and Selection's equivalent) to `prefill.autoRun ?? true`. Consumed
by the `$effect` block in the dialog that fires when the flag is true and the dialog is mounted. Idempotent: the effect
clears the flag first.

**Path pills with overflow collapse**: Each result row's path column renders as a strip of clickable ancestor pills
produced by `PathPills.svelte`. Clicking a pill calls the dialog's `onNavigate(ancestorPath)` callback, which closes the
dialog and navigates the active pane to that ancestor. Pills are **not** in the keyboard Tab order (`tabindex="-1"`):
tabbing through them would break the row's arrow-down keyboard flow inside the virtualized list. They're mouse-only,
with no keyboard equivalent (`⌥←` / `⌥→` stay native move-by-word in the query input). Paths are split strictly on `/`;
macOS and Linux only, no `\` handling.

When the path doesn't fit its column, the middle pills collapse into a single `…` pill. Width is measured with
`@chenglou/pretext` (the same canvas-based measurer the rest of the app uses); the first and last segments stay visible.
Hovering the `…` pill shows a tooltip listing the hidden segments as clickable buttons.

The pill's `onclick` calls `e.stopPropagation()` so it doesn't double-fire the row's `onResultClick`. Svelte 5 delegates
events at the document root, so unit tests assert against the `stopPropagation` spy rather than racing a wrapper DOM
listener.

**Per-row menu**: a right-click on a row calls `onRowMenu(entry)` on the parent, which routes to the existing native
`showFileContextMenu` factory (Open, Reveal in Finder, Copy path, Copy name, …). There is no `…` button and no Actions
column.

## Key shared decisions

**Decision**: Unified query bar with mode chips, not two separate input rows. **Why**: AI prompts and filename patterns
are two ways to ask the same question. Keeping them in separate inputs makes them feel like competing features and
crowds the dialog's top. One `<input>` plus a mode-chip row mirrors Spotlight and Raycast, halves the visual weight, and
lets `⌘1` / `⌘2` / `⌘3` and the placeholder copy carry the mode discriminator. The state-shape collapse (one `query`
plus `mode`, no `aiPrompt` / `namePattern` split) is a permanent simplification.

**Decision**: `MAX_HISTORY_PER_TAB = 100`. **Why**: Not search-specific, but landed in this redesign because the
snapshot store needs an authoritative eviction signal. The cap applies to every volume (local, network, MTP,
search-results) uniformly. 100 is enough for power users who navigate deeply and use `⌘[` for orientation; tightening
below would start to hurt them. The cap is enforced inside `navigation-history.ts::push()`, which returns the dropped
entries so callers (the tab-state manager) can release per-entry resources in one step.

**Decision**: AI mode example chips re-run on click, but recent-items picks do NOT. **Why**: an empty-state example chip
has one meaning ("run this"), so the click IS the explicit AI trigger. A recent entry is different: the user is usually
reaching for a past search to adjust it, so picking one loads it and stops, leaving `⏎` to run. That also keeps an AI
entry from re-billing the user for what they meant as navigation. See § Recent-items dropdown.

**Decision**: `RecentItemsPopover` reuses `$lib/ui/Popover` for positioning + focus trap + Esc-scoped close. **Why**:
it's a sub-overlay-of-an-overlay with the same auto-flip, focus-trap, and "Esc closes only the popover" semantics as the
filter chips. Reimplementing those risks drift; reusing the primitive guarantees the contract covers both popover kinds
via the single `.ui-popover` DOM selector (the same selector the filter popovers render through `FilterPopover`).

**Decision**: Path pills inside result rows are mouse-only and not in the keyboard Tab order, with no keyboard
equivalent. **Why**: Making the pills tabbable inside virtualized rows would break the row's arrow-down keyboard flow:
pressing Down at the end of a row would land on the next row's first pill instead of the next row's primary cell.
Keyboard users navigate the list with arrow keys (cursor row is the keyboard target). The pills carried `⌥←` / `⌥→` as a
keyboard equivalent, but those steal macOS's universal move-by-word from the focused query input (where focus almost
always sits), so the binding was dropped: `⌥←` / `⌥→` now stay native move-by-word and the pill nav is mouse-only. Don't
re-add an `⌥`+arrow folder-nav here; if a keyboard affordance is wanted later, pick a combo with no text-editing meaning
(for example `⌘↑` / `⌘↓`, Finder's enclosing/open keys). Axe's `nested-interactive` rule still flags the structural
nesting on the populated-results audit; we disable that one rule explicitly with a comment pointing here.

**Decision**: AI mode never auto-applies; only Enter / `⌘Enter` / the ⏎ button / example-chip clicks fire it (a
recent-items pick loads without running). **Why**: AI calls cost money (cloud) or RAM + latency (local). Even a fast
model has a per-call cost the user should opt into. Filename and regex modes auto-apply behind the `search.autoApply`
setting (default on, 1,000 ms debounce). The split lives in `scheduleSearch()`'s early-return chain (mode, setting, IME
composition).

Filter-chip-specific decisions (popovers vs inline controls, the always-rendered Pattern chip) live in
`filter-chips/CLAUDE.md`.

## Shared gotchas

**Gotcha**: `stopPropagation()` on every `keydown`. **Why**: Without this, keys propagate to the file explorer behind
the dialog and trigger quick-search or navigation.

**Gotcha**: Don't call the dialog's clear hook from `onDestroy`. **Why**: The dialog's lifecycle (mount on open, unmount
on close) doesn't match the user's mental model of "the search I was working on." Wiping state on unmount turns every
close + reopen into a lost-work moment. The only sanctioned reset path is `⌘N`. If you find yourself wanting to wipe
state from a lifecycle hook, you probably want a user-initiated action instead.

**Gotcha**: status bar stays empty whenever the content area is showing a state message (Searching, No files match,
Loading drive index). The rule: content is the source of truth; duplicating the same string in the status bar reads as
broken. When you add a new content-area state in `QueryResults.svelte`, make sure `getStatusText()` returns `''` for
that state.

An empty bar then COLLAPSES: `.status-bar.is-empty` zeroes its padding and height and makes its top border transparent,
so a running search doesn't end the results well in a bordered strip with nothing in it. Two reasons it collapses
instead of unmounting: the `aria-live="polite"` region has to exist before its content changes to be announced, and a
mount/unmount would change the dialog's height every time the bar found something to say.

**Gotcha**: ⌘⏎ and ⇧⏎ are explicit no-ops in the dialog. Bare Enter is the only key that runs a search or opens the
cursor row (dispatched via `enterAction` per D8). The dialog's `handleModifierShortcuts` swallows both modifier
combinations with `preventDefault` so the bare-Enter handler never sees a modified Enter.

**Gotcha**: The AI's translation overwrites `query` and `mode`. **Why**: We want the bar to show what was searched, not
the natural-language prompt. The original prompt is preserved separately in `lastAiPrompt` (set by `executeAiSearch`
before the IPC call) so the `AiPromptStrip` can render it. Anyone building on top of this should not assume `query`
still contains the user's natural-language input after an AI run; use `getLastAiPrompt()` instead.

**Gotcha**: `nested-interactive` axe rule is explicitly disabled on the populated-results a11y test. **Why**: The row
gains interactive children (the path-pill buttons) inside the `role="option"` row. Tab order is suppressed via
`tabindex="-1"` per spec, but axe still flags the structural nesting. Cleanly fixing it means either dropping the row's
`role="option"` (and surfacing the cursor via a custom mechanism) or hoisting the buttons out of the row's grid cell —
both are out of redesign scope.

## Dependencies

- `$lib/ui/ToggleGroup.svelte` — segmented-control primitive used by `ModeChips`
- `$lib/tauri-commands` — `getRecentSearches` (Search) / future `getRecentSelections` (Selection) via the recent-items
  factory; `showFileContextMenu` (row context menu)
- `@leeoniya/ufuzzy` — fuzzy filtering inside `RecentItemsPopover`
- `$lib/settings` — `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- `$lib/tooltip/tooltip` — chip tooltips (Content chip's "Coming soon" copy, recent-items chip tooltips)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
