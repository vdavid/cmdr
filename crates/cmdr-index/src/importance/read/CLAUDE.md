# Importance read API

`ImportanceIndex` is the ONE way a consumer (the in-app agent, media-ML enrichment, `search/` ranking, the MCP resource)
reaches folder importance. `mod.rs` holds the handle, the typed lookup, the ranked reads, `scored_volume_ids`, and the
recompute subscription. ❌ Don't add a second reader, and ❌ don't take a raw `rusqlite` dep on `importance.db` anywhere
else.

## Must-knows

- **It reads the DB file DIRECTLY and never touches the index registry**, so weights stay queryable OFFLINE after the
  volume unmounts. ❌ Don't route a read through `get_read_pool_for` or gate it on a mount check. A missing or
  never-scored DB reads empty / `None`, ❌ never an error.
- **`lookup` returns typed `WeightLookup::{Scored, Floored(FloorReason), Unscored}`.** The store keeps NO row for a
  floored folder, so `Floored` is derived LIVE from the path; `Unscored` means genuinely not scored. ❌ Don't collapse
  the two — `weight_for` already flattens both to `None` for callers that only want the scalar.
- **`explain` re-scores the STORED signals through the pure scorer**, so a breakdown can't drift from the stored scalar.
  Open the index with the volume kind's `SignalSet` (`scheduler::signal_availability`) or an SMB folder's breakdown
  won't sum to its stored score.
- **`for_each_nonzero_weight` STREAMS `(path, score)`; ❌ never materialize a `path → score` map here.** One measured
  368,043-folder NAS volume costs 58 MB as a map, and each streamed `path` borrows SQLite's row buffer, so a row
  allocates nothing. Floored folders are omitted, so a consumer must treat "absent" as `0.0`.
- **Staleness is first-class, never hidden.** Every result carries `as_of_generation`; the CALLER compares it to
  `recompute_generation()` and caveats. ❌ Don't filter stale rows out or fail on them.
- **Consumers subscribe, they don't poll**: `subscribe(volume_id)` is a `watch<u64>` retaining the last completed
  generation, fired once per pass by the scheduler.

Each call's contract, the offline-read proof, the consumers, and the `importance-tune` dev surface: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
