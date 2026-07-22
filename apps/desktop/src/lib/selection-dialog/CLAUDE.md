# Selection dialog

The "Select files…" / "Deselect files…" dialog. Lets the user select files in the focused pane by a wildcard, regex, or
natural-language prompt (AI mode, cloud only). Second consumer of the shared `QueryDialog` primitive in
`../query-ui/CLAUDE.md`: Search is the first; Selection mirrors its shape. Backend (history store + AI translation)
lives in `apps/desktop/src-tauri/src/selection/CLAUDE.md`.

## Module map

- `SelectionDialog.svelte`: thin wrapper that builds the `QueryDialogConfig` and mounts `QueryDialog`. Owns the AI
  translation IPC, the matcher invocation, the commit-on-Enter path, and the recent-selections write-back.
- `selection-state.svelte.ts`: Selection's module-level `QueryFilterState` singleton + `clearSelectionState()` (the `⌘N`
  reset). `selection-matching.ts`: pure `matchEntries` (glob/regex + size/date predicates). `folder-sampler.ts`: pure
  AI-context sampler. `selection-history-state.svelte.ts`: recent-items factory + apply round-trip.

## Invariants and guardrails

- **Empty pattern with an active filter → match-all on the name; empty pattern and no filters → `[]`.** Filter-only
  queries are valid: `buildMatchQuery` substitutes a match-all glob `*` when the bar is empty but `hasActiveFilter()` is
  true (size ≠ any, date ≠ any, OR `typeFilter` ≠ both), so `≥ 1 MB` with no glob selects every file ≥ 1 MB.
- **Don't reintroduce an empty-pattern early-return in `buildMatchQuery` that ignores the filters,** or the size / date
  / type controls go decorative. The matcher's `compilePattern` still returns `null` on an empty pattern, so the
  wrapper, not the matcher, owns the substitution.
- **Type-filter and folder-size accessors build through the single `buildAccessors()` helper.** Both accessor sites
  (`runQuery` preview AND `commitMatches`) use it, so preview and commit can't disagree on size/type semantics. Don't
  re-inline a second accessor literal. (`getSizeFor` returns `entry.size` for files and `entry.recursiveSize` for
  directories; a folder's `recursiveSize` is `undefined` until the index computes it, so an un-indexed folder can't
  match a size filter, honest, not a bug.)

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
  `applyIndices(hasParent=true)` already skips index 0, the dialog's preview has to match). The wrapper's
  `dropParentIndex` helper handles this. Pinned by the "drops the synthetic `..` parent" test.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
