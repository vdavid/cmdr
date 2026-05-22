# Selection dialog plan: review round 2

Reviewer: fresh-eyes Opus agent verifying round-1 fixes. Round 1 found 8 blockers, 17 important gaps, 12
nice-to-haves; this pass tags each, then scans the revision for newly introduced issues.

## Status table

### Blockers

| Tag | Status | Notes |
|---|---|---|
| B1 (Ark UI claim about `SearchModeChips`) | ✅ Resolved | Plan now states only `SettingToggleGroup` uses Ark; the bespoke tablist behavior is acknowledged as 246 lines and ported verbatim under `semantics="tabs"`. |
| B2 (toggle-group vs tab-strip semantics) | ✅ Resolved | `semantics: 'tabs' \| 'toggles'` prop is coherent: each branch renders its own ARIA shape; shared CSS chrome lives at the component level. See "New findings" below for one detail about Ark CSS integration. |
| B3 (snapshot-pane accessor) | ⚠️ Partial | The Match-semantics section, the Risk register R7, and the matching-test mandate all correctly say `entry.name`. But M6 § "What" line 973 still reads "the dialog's match runs against `entry.path`". That line contradicts the B3 resolution and will mislead the agent implementing the pane-side plumbing. |
| B4 (`gpt-5.5` hard-codes) | ✅ Resolved | All assertion-style references are gone; the one remaining mention (R3, "David's curl example used `gpt-5.5`") is historical context, correctly framed. |
| B5 (`=`/`-` key binding) | ✅ Resolved | Keyboard contract section and M7 both bind `event.key === '+'` and `event.key === '-'`, guard against `metaKey/altKey/ctrlKey`, intentionally do NOT test `shiftKey`, and a unit test is mandated. |
| B6 (M4 line-count target) | ✅ Resolved | "Wrapper's size is whatever Search-specific glue costs; no line-count target." `lastDialogEvent` ownership is also pinned. |
| B7 (search-only state in shared factory) | ✅ Resolved | Core / extras split is explicit, with the exact field list (`scope`, `excludeSystemDirs`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind` in `search-extras-state.svelte.ts`). See "New findings" for one verification gap on `recordAiTranslation`. |
| B8 (`recordAiTranslation` for Selection) | ✅ Resolved | Decision-log entry states `recordAiTranslation` writes only to `handTyped.{filename\|regex}` on the core factory; the wrapper writes its own extras. Selection's modes are explicitly AI / Filename / Regex. |

### Important gaps

| Tag | Status | Notes |
|---|---|---|
| G1 (capability files) | ✅ Resolved | M5 lists the six commands per window with verification steps. |
| G2 (specta exclusion list) | ✅ Resolved | M5 explicitly says the new commands are NOT debug, no exclusion entry needed. |
| G3 (CommandScope value) | ✅ Resolved | M7 specifies `'Main window/File list'`, do NOT add a new scope. |
| G4 (platform menu files) | ✅ Resolved | M8 names `macos.rs::build_menu_macos` and `linux.rs::build_menu_linux` plus `menu_items.rs` and `mod.rs::menu_id_to_command` / `command_id_to_menu_id`. |
| G5 (Decision log entry for moving Select all out of Edit) | ✅ Resolved | Decision-log line documents the deliberate UX choice. |
| G6 (`applySelectionHistoryEntry`) | ✅ Resolved | M7 specifies it, lists fields restored, mandates a unit test. |
| G7 (AI provider gating) | ✅ Resolved | Cloud-only gate, live `onSpecificSettingChange('ai.provider', …)` subscription, tooltip copy. See "New findings" for one UX gap about mid-dialog provider switch. |
| G8 (E2E spec under 1s, AI excluded) | ✅ Resolved | M7 explicitly excludes AI from the spec; manual MCP smoke for AI in M11. |
| G9 (missing test surfaces) | ✅ Resolved | IPC contract tests, proptest on `matchEntries`, parser unit tests, offline integration test, real-LLM eval test, `lastDialogEvent` ownership unit test all called out. |
| G10 (CLAUDE.md split sheet) | ⚠️ Partial | The split sheet is present and structured, but compresses dozens of R3 polish items, gotchas, and key decisions into one-line "tag per item" entries. An agent executing M3 will still need judgment calls on which exact text moves to which file. See "New findings" for a concrete suggestion. |
| G11 (Linux keystroke test) | ✅ Resolved | M7 specifies an e2e-linux spec that opens via `dispatchMenuCommand`, explicitly NOT testing key-press on Linux. |
| G12 (RecentItemsFooter adapter shape) | ✅ Resolved | Adapter signature locked: `{ label, tooltip, mode, ageLabel, ariaLabel }`. |
| G13 (title bar Tab order) | ✅ Resolved | "No close button; Escape is the only close path. Title bar is not in the Tab order." |
| G14 (`runHintCopy`) | ✅ Resolved | Locked into `QueryDialogConfig`. Search passes "Press Enter to search", Selection passes "Press Enter to filter". |
| G15 (`aiContext` timing) | ✅ Resolved | Once per AI translation, snapshot at dialog open, no refresh on pane change. |
| G16 (cross-snapshot delete + Selection) | ✅ Resolved | M7 mandates commit-time matching against the snapshot's current entries, plus an integration test. |
| G17 (em dashes / passive voice) | ❌ Not resolved | 5 em dashes remain in the plan (all inside the CLAUDE.md split sheet, lines 1243, 1244, 1256, 1257, 1259). Decision log entry "(Resolves B7. Rationale: …)" reads passive-ish. |

### Nice-to-haves

| Tag | Status | Notes |
|---|---|---|
| N1 (`ModeChips` over `QueryModeToggleGroup`) | ✅ Resolved | Renamed throughout. |
| N2 (`lib/query-ui/` over `lib/query/`) | ✅ Resolved | Kept `query-ui`. |
| N3 (`runOnMount` default for Selection) | ⚠️ Partial | The plan says Selection ignores `runOnMount` but doesn't say it defaults to undefined/no-op in the wrapper. Minor; the implementing agent will figure it out. |
| N4 (`cargo mutants` scope) | ✅ Resolved | M10 includes `query_builder.rs`. |
| N5 (empty-state copy pairing) | ✅ Resolved | Examples now pair AI intent with non-AI equivalents; the size-only third slot is honestly flagged as "use the filter chip". |
| N6 (NOT using `ModalDialog`) | ✅ Resolved | Title-bar section repeats the decision. |
| N7 (Cargo deny re-run) | ✅ Resolved | "No `cargo deny` re-run needed: no new Cargo deps." |
| N8 (`maxCount = 0` disables) | ⚠️ Partial | M5 says "cap=0 disables persistence" in the test list, but the Decision log doesn't pin it explicitly. Carried implicitly by mirroring search's behavior; fine. |
| N9 (AI eval feature flag) | ✅ Resolved | M5 says "Reuse the existing AI eval feature flag if one exists. Check `Cargo.toml`." |
| N10 (run cadence) | ⚠️ Partial | M11 covers `--include-slow`; M10's "Polish and bug-hunt" runs default suite and the slow suites individually but doesn't explicitly say `--include-slow` runs there too. Minor; M11 is the gate. |
| N11 (M2 call-site count) | ⚠️ Partial | The Decision log says "~471 identifier usages per a fresh grep" — corrected from "~100". But R1 in the Risk register still reads "~100 import sites". Two numbers in the same plan, different by 5x. Pick one. |
| N12 (drop ToggleGroup tests if B2 path a chosen) | ✅ Resolved | B2 went path (c)-ish; the tests are needed. |

## New findings

### 🔴 Blockers (must fix before execution)

**NB1: M6 still says `entry.path` for snapshot panes, contradicting B3.**
Location: M6 § "What" line 973: "For `search-results://` panes: the dialog's match runs against `entry.path`, but
`applyIndices` still operates on indices into the snapshot's `entries[]`."
This contradicts the B3 resolution (which mandates `entry.name`) and the Risk register R7. An agent implementing M6
will plumb the wrong accessor through `FilePane.applyIndices`, and the matching unit test mandated for `entry.name`
in `selection-matching.test.ts` will pass while the running app silently matches the wrong string. Replace
"`entry.path`" with "`entry.name` (which is the displayed friendly path on snapshot panes; see § Match semantics)"
on line 973.

**NB2: R1 in the Risk register contradicts the Decision log on M2 churn.**
Decision log line 507: "~471 identifier usages per a fresh grep". Risk register R1 line 1268: "~100 import sites".
The Decision log number is correct (it cites the grep). R1's "~100" is stale from the round-1 plan. Update R1.

### 🟡 Important gaps

**NG1: Mid-dialog AI provider switch UX is undefined.**
G7 is resolved at the binding-time level (the chip appears/disappears via `onSpecificSettingChange`). But: what
happens if the user opens Selection in AI mode, then the provider gets switched off in another window? The plan
says "the chip stays hidden", but the dialog's `state.mode` is still `'ai'`. The mode chip row would render with
AI absent; what mode is active? Pick: (a) auto-fall-back to Filename mode and surface the AI prompt → Filename
buffer (B8's pattern), (b) close the dialog with a toast, or (c) keep the bar in "stuck" AI mode with the AI chip
invisible (worst option; user confusion guaranteed). Recommend (a) for parity with Search's existing
"`autoFallbackToFilename` when AI gets disabled mid-session" gotcha (see `lib/search/CLAUDE.md` § Gotchas).
Lock in M7's spec.

**NG2: The CLAUDE.md split sheet, while a step forward, still requires judgment from the executing agent.**
G10 partial. The sheet tags entries with shorthand like "B1/B5/U1/U2/U3/U4/U5/U7 → query-ui; B2/B3/B4/B6/U6/U8/T1 →
search" without saying what those item IDs actually contain. An agent executing M3 must open
`lib/search/CLAUDE.md`, decode the item IDs against the current text, then move per the tags. Two minor risks:
(a) the agent miscategorizes a polish item, (b) the agent loses cross-references between items. Mitigation:
either (a) M3's first commit creates an "X-to-Y mapping" file inside the plan as the agent reads through the
source (essentially elaborating this table to one row per decision/gotcha), or (b) require a single human pass at
M10 that diffs `lib/search/CLAUDE.md@before` vs `lib/search/CLAUDE.md@after + lib/query-ui/CLAUDE.md@after` to
confirm zero content loss. Option (b) is cheaper.

**NG3: `recordAiTranslation` refactor scope underspecified.**
The Decision-log entry says "Selection's wrapper does NOT call any Search-extras setters from
`recordAiTranslation`; the AI label and the AI pattern (for Search's Pattern chip) are the wrapper's job to
populate via its extras module." But the existing `recordAiTranslation` in `search-state.svelte.ts` writes
`lastAiPattern`, `lastAiPatternKind`, AND `lastAiLabel` in one shot (line 373-375). For the split to work as
the plan says, the M2 refactor must split this function: the core's version only updates `handTyped[mode]`, and
Search's extras module exposes a separate `recordAiLabelAndPattern({ pattern, kind, label })` that the wrapper
calls right after the core call. The plan implies this but doesn't say it. M2 should add an explicit bullet:
"Split `recordAiTranslation`: core writes only `handTyped`; extras writes `lastAiPattern`/`lastAiPatternKind`/
`lastAiLabel`. Both functions are called from Search's wrapper in sequence; Selection's wrapper only calls the
core version."

**NG4: G17 em-dash fix incomplete.**
5 em dashes remain in the CLAUDE.md split sheet (lines 1243, 1244, 1256, 1257, 1259). They're in table cells, so
trivially fixable: replace ` — ` with `: ` or a colon-led phrase. Not blocking execution but the plan is the
style anchor and round-1 explicitly flagged this.

### 🟢 Nice-to-haves

**NN1: Ark UI `ToggleGroup.Item` CSS scoping.**
The plan says "shared visual CSS at the component level so both ARIA shapes render identically. Use the existing
CSS tokens that `SettingToggleGroup.svelte` already defines (border, radius, hover, active background)". Ark
generates its own classnames at runtime via its part attributes. The implementing agent will need to verify the
CSS selectors target Ark's `[data-part="item"]` and `[data-state="on"]` attributes (toggles branch) and the
bespoke buttons (tabs branch). Add a sub-bullet to M1: "verify the shared CSS selectors target both Ark's
data-part attributes and the bespoke `<button>` shape, screenshot diff Settings before/after to confirm no visual
regression."

**NN2: `lastDialogEvent` write set for consumer-side mutations.**
The plan locks `QueryDialog` as the writer for `'opened'`, `'query-edited'`, `'filter-edited'`,
`'results-arrived'`, `'cursor-moved'`. But there are consumer-driven events too: Search's snapshot promotion
triggers a state change; Selection's `applyIndices` runs before close. Are those events sources? Probably not
(they only fire on commit, after which the dialog closes), but spell it out: "no consumer-side writes to
`lastDialogEvent`; commit-time events don't need to be sequenced because the dialog is closing."

**NN3: N3 default for `runOnMount`.**
M4's config defines `onMount?: () => void | Promise<void>` — but where does `runOnMount` (the prefill hook) live?
The plan uses `runOnMount` and `onMount` somewhat interchangeably. M4's `QueryDialogConfig` shows `onMount?` not
`runOnMount?`. Verify these are the same thing or document the difference. The state factory section (line 654)
lists `runOnMount` as a state field, distinct from `onMount` as a lifecycle hook. Spec'd, just confusing —
consider renaming the state field to `pendingPrefill` or similar to avoid the name collision.

**NN4: Style: a couple of latinisms.**
"e.g." appears once at line 798 ("e.g. 'min(1080px, 80vw)'"). Per David's writing style, replace with
"for example". Trivial.

## Verdict

**REVISE.**

The plan made huge progress: all 8 blockers from round 1 are resolved or close to it (B3 is the one straggler).
G1-G16 are clean. Decision-log entries explain the trade-offs honestly. The CLAUDE.md split sheet, while still
requiring some agent judgment, is genuinely a useful guide instead of "M10 sweeps everything later." This is
1-2 hours of revision shy of execution-ready.

Must-fix before execution:

1. **NB1**: Fix M6's `entry.path` → `entry.name` to match B3.
2. **NB2**: Fix R1's "~100 import sites" → "~471 identifier usages" to match the Decision log.

Should-fix (not blockers, but worth a 30-min revision pass):

3. **NG1**: Spec the mid-dialog AI-provider-off behavior in M7 (recommend Filename-mode fall-back to mirror Search).
4. **NG3**: Add an explicit M2 bullet splitting `recordAiTranslation` between core (`handTyped`) and Search-extras
   (`lastAiPattern` / `Kind` / `Label`).
5. **G17/NG4**: Sweep the 5 remaining em dashes from the CLAUDE.md split sheet table; replace "e.g." at line 798.
6. **NG2**: Decide between (a) elaborating the split sheet to one row per decision, or (b) requiring an M10
   before/after diff check. (b) is cheaper.

Everything else (NN1-NN4 and the various N# partials) is nice-to-have polish that the implementing agents can
handle in the moment without blocking on the planner.

After those fixes, the plan ships.
