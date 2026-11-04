# Search module (host)

The desktop host of the drive search. The core (query model, matcher, ranking, the arena and its scan, the seam) is
`crates/cmdr-search` (`crates/cmdr-search/CLAUDE.md`); its modules keep their `crate::search::…` paths through
`mod.rs`'s re-export. **One volume per search, enforced at the API** (`resolve_target`): ❌ no fan-out, the only way a
search can silently omit a drive. Scope routes the volume (or the caller placed it: `run_live_collected`'s
`on_volume`); unscoped means boot volume.

`execute.rs` routes index/live search over `execute/coverage.rs`; `index_source.rs` answers the seam from SQL;
`index/load.rs` reads arenas; `volumes.rs` caches them; `live.rs` holds the one run registry and pumps the walk into the crate's `ResultStream` (`live/CLAUDE.md`);
`history.rs` owns recents; `ai/` translates NL (`ai/CLAUDE.md`).

## Must-knows

- **`search/` consumes `cmdr-index` ONE WAY** — ❌ no matcher, query, or search type inside `cmdr-index`, and ❌ no
  `cmdr-index` in `cmdr-search`.
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
  `summarize_query_for_diagnostics` logs the pattern's shape only; `summarize_query` stays literal for MCP
  `interpreted_query`. A resolved scope can't distinguish a typo from not-yet-walked ground.
- **A source read error is `IndexUnreadable`** in a live run (`covered_half_for_run`), ❌ never `Query` with SQL text.
- **`prepare_search_index`'s `loading` says whether an event is COMING**; `loading: false, ready: false` is the terminal
  "no index here", or a machine that declined indexing waits forever.
- **Memory is the design constraint**: an idle arena drops 30 s after its dialog or MCP call; the load splits across
  eight rowid ranges and its merge is the memory peak (DETAILS § Loading in parallel).
- **The goldens** (`execute/tests/goldens.rs`) pin today's answers: a moved golden is a bug, ❌ never a re-record.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
