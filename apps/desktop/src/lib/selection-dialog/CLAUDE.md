# Selection dialog

The "Select files…" / "Deselect files…" dialog. Lets the user select files in the focused pane by a wildcard, regex, or
natural-language prompt (AI mode, cloud only). Second consumer of the shared `QueryDialog` primitive in
[`lib/query-ui/`](../query-ui/CLAUDE.md) — Search is the first; Selection mirrors its shape.

Backend lives in [`src-tauri/src/selection/`](../../../src-tauri/src/selection/CLAUDE.md) (history store + AI
translation). The shared dialog primitives live in [`lib/query-ui/`](../query-ui/CLAUDE.md).

Dialog dimensions: `max-width: min(720px, 60vw)` (narrower than Search's `min(1080px, 80vw)` because Selection lists
have no path column and shouldn't dominate the viewport). `max-height: 80vh`.

## Files

| File                                | Purpose                                                                                                                                                                                                                         |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SelectionDialog.svelte`            | Thin Selection-specific wrapper. Builds the `QueryDialogConfig` and mounts `QueryDialog`. Owns the AI translation IPC, the matcher invocation, the commit-on-Enter path, and the recent-selections IPC write-back.              |
| `folder-sampler.ts`                 | Pure: `sampleFolderNames(names, cursorIndex, max=240)`. Returns first 200 + 20 around the cursor + last 20, deduped and capped. Used as AI context.                                                                             |
| `folder-sampler.test.ts`            | Pins the bucket math, the cursor-band clamp at both ends, dedup, empty folder, and the custom cap.                                                                                                                              |
| `selection-matching.ts`             | Pure: `matchEntries(accessors, total, query) → number[]`. Compiles the glob to a JS RegExp (anchored, mirrors the Rust `glob_to_regex` in `src-tauri/src/search/query.rs`), composes pattern + size + date predicates with AND. |
| `selection-matching.test.ts`        | Glob / regex / case-sensitive / size / date / snapshot-pane accessor / empty pattern / bad regex / stress invariants (sorted, no dups, in-range).                                                                               |
| `selection-history-state.svelte.ts` | Instantiates the recent-items factory for Selection (uses `getRecentSelections` and friends). Exports `applySelectionHistoryEntry` for restoring a recent entry into the dialog state.                                          |
| `selection-history-state.test.ts`   | Pins the apply round-trip: mode, query, caseSensitive, size filter, date-filter clear, hand-typed buffer carry-through, AI-prompt-as-query persistence.                                                                         |
| `SelectionDialog.svelte.test.ts`    | Title-per-mode, commit-on-Enter, R7 banner visibility, mid-dialog AI-provider fallback.                                                                                                                                         |
| `SelectionDialog.a11y.test.ts`      | Tier-3 axe-core audit across Select / Deselect / AI-on / snapshot-pane variants.                                                                                                                                                |

## Wiring

```
+page.svelte
  ├─ showSelectionDialog: 'add' | 'remove' | null
  ├─ selectionDialogSnapshot: { entries, cursorIndex, isSnapshotPane }
  └─ <SelectionDialog mode entries cursorIndex isSnapshotPane onCommit onClose />
        ├─ creates its own QueryFilterState (separate factory instance from Search's)
        ├─ matchEntries(accessors, total, query) at runQuery time
        ├─ translate_selection_query IPC (AI mode, cloud only)
        └─ on commit: explorerRef.applyIndicesToFocusedPane(indices, mode)
```

## Match semantics

The matcher runs against an `accessors.getNameFor(i)` callback. The wrapper passes `entry.name` for both regular panes
and `search-results://` snapshot panes (on snapshot panes, `entry.name` IS the friendly full path — see
[`lib/file-explorer/DETAILS.md`](../file-explorer/DETAILS.md) § "Search-results virtual volume"). The matcher itself
doesn't care which kind it's running against.

Glob translation matches the Rust side: `*` → `.*`, `?` → `.`, regex metacharacters escaped, anchored with `^…$`. The JS
regex engine is what does the matching (Selection has no Rust IPC for the match itself — it's microseconds in JS against
a few hundred entries). Bad regex (`SyntaxError`) → `[]`. Empty pattern → `[]`.

## Folder sampling

The AI translator needs a representative folder sample. We send up to 240 names per call:

- 0–200 entries: all of them.
- 201+ entries: first 200, plus 20 around the cursor (cursor − 10 .. cursor + 9, clamped), plus the last 20.

Deduped, deterministic (no `Math.random`). The sample is snapshotted at dialog open and NOT refreshed on mid-dialog
focused-pane change (per plan G15).

## Apply-on-commit

The matcher runs at COMMIT time (Enter / button click), not at preview time. The preview list above the footer shows the
matching rows live as the user types; pressing Enter re-runs the matcher one more time against the open-time snapshot
and hands the matched indices to `applyIndicesToFocusedPane(indices, mode)`. We do NOT mutate the focused pane's
selection while the user previews — matches Total Commander's "+" / "-" behaviour.

## Mid-dialog AI-provider fallback

Selection's AI is cloud-only (per the plan: small local models can't reliably handle a 200+-name sample plus the
structured prompt). When the user has the dialog open in AI mode and flips the provider off in another window, the
wrapper's `$effect`:

1. Reads the current AI prompt from the bar.
2. Writes it to `handTyped.filename`.
3. Calls `switchMode('filename')`, which swaps the bar's value into the filename buffer's restored value (the prompt we
   just put there).

The user keeps their words and can refine or delete them. QueryDialog's own fallback effect
(`!config.aiEnabled && mode === 'ai'`) fires too but is a no-op by then — mode is already `'filename'`.

## Keyboard contract

The Selection dialog inherits the shared QueryDialog keyboard contract (⌘N reset, ⌘H popover, ⌘1/⌘2/⌘3 mode switch,
⌥A/F/R mode chip shortcuts, ⌥⏎ primary action, bare Enter ownership swap, IME guard, etc.).

The bare `+` / `-` keys that open the dialog from a file pane are NOT a QueryDialog concern — they live on the focused
pane via `FilePane.handleSelectionDialogKey`, which delegates to the pure classifier in
[`pane/selection-dialog-keys.ts`](../file-explorer/pane/selection-dialog-keys.ts). The filter is
`!metaKey && !altKey && !ctrlKey` (shiftKey is intentionally NOT filtered — on US QWERTY, Shift+= IS how the user
produces `event.key === '+'`).

## Snapshot panes

For `search-results://` panes, the matcher runs against `entry.name`, which on snapshot panes IS the displayed friendly
path (home folder shown as `~`, mid-truncated for display), NOT the raw `entry.path`. The dialog renders a small banner
above the chip strip ("Matching what is shown in the list (the full path).") so the user knows what the matcher sees.
The R7 banner is driven by `config.noticeBanner` (the same prop Search uses for its own banners).

`applyIndices` on a snapshot pane operates on indices into the snapshot's `entries[]` exactly as for regular panes — no
special-casing at the pane API layer.

## Gotchas

- **AI runs reset the other-kind hand-typed buffer and the size + date chips before applying the new translation.**
  `buildMatchQuery` in AI mode picks whichever of `handTyped.regex` / `handTyped.filename` has content (regex first).
  Without the reset, a previous AI run's pattern of the opposite kind, or a previous AI run's size/date filter that the
  new run didn't return, would silently shadow the new translation. The hand-typed value the user actually typed under a
  non-AI mode is also wiped on each AI run; that's the right call because the user invoked AI again, expecting the AI's
  filter set rather than a merge with stale manual tweaks. Pinned by the "a second AI run does not let a leftover
  buffer" test in `SelectionDialog.svelte.test.ts`.
- **The synthetic `..` parent entry at snapshot index 0 is dropped from matches.** On regular panes,
  `FilePane.getEntriesSnapshot` prepends a synthetic entry named `..` so indices align with the pane's selection state.
  A pattern like `*` matches it, but the result count and the rows shown must drop it (the commit path's
  `applyIndices(hasParent=true)` already skips index 0 — the dialog's preview has to match). The wrapper's
  `dropParentIndex` helper handles this. Pinned by the "drops the synthetic `..` parent" test.

## Decisions

- **Selection runs the matcher in JS, not via a Rust IPC.** The matcher iterates hundreds of entries, not millions; the
  JS regex engine is already there; an IPC would add network-round-trip latency for zero benefit.
- **AI is cloud-only.** Small local models (4K–8K context) can't reliably fit a 200+-name sample plus the structured
  prompt and a parseable response. When `ai.provider !== 'cloud'`, the AI chip is hidden.
- **Apply on commit, not live during preview.** Live-apply would mutate the focused pane's selection as the user typed;
  undoing on Esc would be its own engineering problem. Apply on commit is simple, predictable, and matches Total
  Commander's behaviour.
- **Recent-selection entries are added on commit, not on auto-apply.** Same gate as Search's "Open in pane" rule:
  history stays signal-rich (results worth acting on) instead of keystroke-noisy.

## Dependencies

- `$lib/query-ui/QueryDialog.svelte` — the shared orchestrator the wrapper mounts.
- `$lib/query-ui/query-filter-state.svelte` — the cross-consumer state factory (Selection creates its own instance).
- `$lib/query-ui/recent-items/recent-items-state.svelte` — the recent-items factory.
- `$lib/tauri-commands` — `translateSelectionQuery`, `getRecentSelections`, `addRecentSelection`,
  `removeRecentSelection`.
- `$lib/file-explorer/types` — `FileEntry`, the pane snapshot shape.
- `$lib/settings` — `getSetting('ai.provider')`, `onSpecificSettingChange`.
