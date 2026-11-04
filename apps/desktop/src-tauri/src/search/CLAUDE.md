# Search module

In-memory filename search + AI query translation. **One volume per search, enforced at the API** (`resolve_target`):
❌ no fan-out, the only way a search can silently omit a drive. Scope routes the volume (or the caller placed it: `run_live_collected`'s `on_volume`); unscoped means boot volume.

`execute.rs` routes index/live search over `execute/coverage.rs`; `live.rs` owns runs and `ResultStream`
(`live/CLAUDE.md`). `engine.rs` scans `index.rs`'s per-volume arena; matcher/excludes/ranking judge rows; types/query
hold data/operations; `history.rs` owns recents; `ai/` translates NL (`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB), `types.rs` stays logic-free, `search/` consumes
  `indexing/` ONE WAY — ❌ no matcher, query, or search type inside `cmdr-index`.
- **One matcher and one exclusion set, two evaluators** (arena scan = ancestor IDs, live walk = the entry's own PATH).
  ❌ Never re-derive case folding or NFD normalization elsewhere: that fork is how an unindexed drive answers
  differently. ❌ Neither carries directory sizes or the include-root filter.
- **The broad-query guard is per evaluator**: a refusing walk takes the whole RUN with it, ❌ never answering from the
  index alone.
- **A stale root arena is SERVED**, refreshed in the background: ❌ never reload-on-mismatch. The ONE exception, an
  arena that can't honor its coverage answer, CATCHES UP (appends created rows), ❌ never reloads whole, or the NEXT
  query silently returns fewer. `LoadedVolume::honors`: equal tokens or rows READ after the answer, ❌ never token alone.
- **Superseding a run ≠ cancelling it**: events stop, the walk runs on. Cancel is the dialog close (which SPARES
  `keep_run_id`), Escape, or quit — ❌ never the arena idle-drop, `RunOrigin::Dialog` only.
- **Non-root indices are mount-relative**: PREFIX the mount root onto read paths, STRIP it from scopes. Mount root is
  `location::resolve::volume_root`'s (live registry first, else the `volume_path` meta), ❌ never assume the meta is set.
  ❌ Never `realpath` a NAS scope (smbfs makes it NFD): `Target::canonical_scope`.
- **Honesty is TYPED, diagnostics are structural**: `uncovered_scopes`, `unresolved_scopes`, and `SearchRunCoverage`.
  `summarize_query_for_diagnostics` logs the pattern's shape only, and scope roots / exclusions as keyed
  `path=` / `dir=` fields a report tokenizes; `summarize_query` stays literal for MCP `interpreted_query`. A resolved scope can't distinguish a typo from not-yet-walked ground.
- **`prepare_search_index`'s `loading` says whether an event is COMING**; `loading: false, ready: false` is the terminal
  "no index here", or a machine that declined indexing waits forever.
- **A directory's size filter applies BEFORE ranking** (`dir_sizes_for`), ❌ never after, and ❌ never fall back to "no
  map" on a read error — the engine reads that as "no filter". DETAILS § Directory size filters.
- **Memory is the design constraint**: arena-allocated names (❌ no owned `String`s), importance keyed on `hash_path`,
  ranking per MATCH and so top-k. An idle arena drops 30 s after its dialog or MCP call.
- **The arena is sorted by `id`; `index_of_id` binary-searches it.** ❌ No `id_to_index` map (~143 MB), and ❌ no
  fixture building rows out of id order: the loader can't produce one and search answers wrong on it. The load splits
  across eight rowid ranges and its merge is the memory peak. DETAILS § Loading in parallel, § Finding a row by id.
- **A `SearchEntry` is 40 bytes and stays 40 bytes** — one per file, so a byte is megabytes. `size` and `modified_at`
  are `OptU64` (`u64::MAX` sentinel): ❌ never widen back to `Option<u64>`, ❌ never compare against the sentinel
  (`.get()` is the only read), ❌ never collapse `None` into `0` — a NULL `logical_size` is a hardlink-deduped row, not
  an empty file. DETAILS § The arena row.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
