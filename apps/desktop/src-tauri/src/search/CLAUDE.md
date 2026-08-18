# Search module

In-memory filename search + AI query translation. **One volume per search, and that's the CEILING, enforced at the API**
(`resolve_target`), not just the UI: ❌ no fan-out, it's the only way a search can silently omit a drive. A scope routes
to the volume that owns it; unscoped means the boot volume.

`execute.rs` routes and runs the index-only search, `execute/live_run.rs` the live one (dialog and MCP) over
`execute/coverage.rs`'s model; `live.rs` the run registry and `ResultStream` (events and the one-shot fold:
`live/CLAUDE.md`); `engine.rs` scans the arena (`index.rs`, per volume via `volumes.rs`); `matcher.rs`, `excludes.rs`,
`ranking.rs` judge and order a row; `types.rs` / `query.rs` the data, `history.rs` the recent-searches entry over `crate::recents`, `ai/` NL translation
(`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB), `types.rs` stays logic-free, `search/` consumes
  `indexing/` ONE WAY — ❌ no matcher, query, or search type inside `cmdr-index`.
- **One matcher and one exclusion set, two evaluators** (arena scan = ancestor IDs, live walk = the entry's own PATH).
  ❌ Never re-derive case folding or NFD normalization elsewhere: that fork is how an unindexed drive answers
  differently. ❌ Neither carries directory sizes or the include-root filter.
- **The broad-query guard is per evaluator**: a refusing walk takes the whole RUN with it, ❌ never answering from the
  index alone.
- **A stale root arena is SERVED**, refreshed in the background: ❌ never reload-on-mismatch, it costs 2.6 s per search.
  Decision 12 is the ONE exception — an arena that can't honor its coverage answer is rebuilt, or the NEXT query
  silently returns fewer. `LoadedVolume::honors`: equal tokens, or a load STARTED after the answer — ❌ never the token
  alone.
- **Superseding a run ≠ cancelling it**: events stop, the walk runs on. Cancel is the dialog close (which SPARES
  `keep_run_id`), Escape, or quit — ❌ never the arena idle-drop, and `RunOrigin::Dialog` only, so an agent's run can't
  silence a person's.
- **Non-root indices are mount-relative**: PREFIX the mount root onto read paths, STRIP it from scopes. Mount root is
  the `volume_path` meta OR the live registry, ❌ never assume the meta is set; routing picks the live id when one NAS
  has two DBs.
- **Honesty is TYPED, ❌ never a string match**: `uncovered_scopes`, `unresolved_scopes` (❌ never "doesn't exist" — it
  can't tell a typo from a not-yet-walked folder), and `SearchRunCoverage`, where `walk: Completed` ≠ exhaustive.
- **`prepare_search_index`'s `loading` says whether an event is COMING**; `loading: false, ready: false` is the terminal
  "no index here", or a machine that declined indexing waits forever.
- **A directory's size filter applies BEFORE ranking** (`dir_sizes_for`), ❌ never after, and ❌ never fall back to "no
  map" on a read error — the engine reads that as "no filter". DETAILS § Directory size filters.
- **Memory is the design constraint**: arena-allocated names (❌ no owned `String`s), importance keyed on `hash_path`,
  ranking per MATCH and so top-k.
- **A `SearchEntry` is 40 bytes and stays 40 bytes** — one per file on the volume, so a byte is megabytes. Its `size`
  and `modified_at` are `OptU64`, a `u64::MAX`-sentinel encoding: ❌ never "simplify" either back to `Option<u64>`
  (16 B for a value needing 8), and ❌ never compare against the sentinel — `.get()` is the only read. `None` is
  MEANINGFUL in both (a NULL `logical_size` is a hardlink-deduped row, not a zero-byte file), so ❌ never collapse it
  into `0`. DETAILS § The arena row.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
