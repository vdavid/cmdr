# Media store — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## Schema

`SCHEMA_VERSION` is a disposable-cache version: a mismatch delete-and-recreates `media-{volume_id}.db`. It's `4` (v2
added the tag + embedding tables and the FTS `source` column; v3 added the `media_clip_embedding` table + the
`media_status.clip_stamp` column for CLIP semantic search — `../clip/DETAILS.md`; v4 is the "small at NAS scale" bump —
f16 embeddings + integer-id keying, § The f16-embedding + integer-id decisions). A bump re-enriches every beta user's
cache on next launch (Vision recompute only, no re-download — an accepted disposable-cache cost). Objects:

- `media_file` — `(id INTEGER PRIMARY KEY, path TEXT UNIQUE COLLATE platform_case)`: the identity table (plan M4). Each
  path is stored ONCE here; every other table keys on the integer `file_id`, and the reads join back to a path so the
  Rust layer stays path-addressed. A rename is a one-row `UPDATE media_file.path` (`MediaWriter::rename_path`).
- `media_status` — `WITHOUT ROWID`, `file_id INTEGER PRIMARY KEY`; `mtime`, `size` (with the path, the
  `(path, mtime, size)` staleness key); `media_kind` + `state` (typed TEXT tokens, `sqlite3`-inspectable, parsed back to
  typed enums); `engine_version` (the combined analyze provenance stamp, `../backend/DETAILS.md`, so an OS upgrade to
  the OCR engine, tag taxonomy, or feature-print model re-runs analysis even on an unchanged file — data-COVERAGE, not
  data-safety, since the derived data is disposable); `clip_stamp` (the CLIP-side staleness key).
- `media_ocr` — a **standalone** FTS5 table (`file_id UNINDEXED, source UNINDEXED, text`, tokenizer
  `unicode61 remove_diacritics 2`). Not external-content: external content would sync via triggers off another table's
  integer rowid; a standalone table keyed by an UNINDEXED `file_id` keeps enrichment and GC a simple `WHERE file_id = ?`
  delete with no trigger machinery to desync. It holds up to two rows per file: the OCR text (`source='ocr'`) and the
  space-joined tag labels (`source='tag'`), so a keyword search matches **tags alongside OCR**. Created via
  `CREATE VIRTUAL TABLE … USING fts5`, which doubles as the FTS5 availability guard (a `bundled` build without FTS5
  fails there — Decision 2's build-flag worry is closed, `agent/store` proves it).
- `media_tags` — `(file_id, label, score)` with an index on `file_id` and on `label`: the STRUCTURED tags for tag-score
  filtering (`images_with_tag(label, min_score)`), distinct from the folded FTS keyword index above.
- `media_embedding` — `WITHOUT ROWID`, `(file_id PRIMARY KEY, dims, vector BLOB)`: the image feature-print embedding as
  a little-endian **`f16`** BLOB (`encode_embedding`/`decode_embedding`; `dims` = element count). The vector store's
  load source (`../vector/DETAILS.md`).
- `media_clip_embedding` — same shape (`file_id`, `dims`, `f16` `vector`), the CLIP image embedding in its SEPARATE
  vector space.
- `meta` — `schema_version` only.

The `needs_enrichment` staleness predicate is `(path, mtime, size)` + the analyze stamp: stale when there's no row, or
when `(mtime, size)` changed, or when the stamp changed. State is deliberately excluded from the key so a failed file
isn't re-hammered every completed scan; a real file change re-tries it. A successful `upsert` resolves the path to its
`media_file` id (creating it if new), then writes `media_status` + the OCR/tag FTS rows + `media_tags` +
`media_embedding` in ONE transaction (clearing each prior row first, so a re-enrichment leaves nothing stale); a failure
clears them all and records only the `Failed` status. GC/prune delete every `file_id`-keyed child plus the `media_file`
row.

## The f16-embedding + integer-id decisions (plan M3 + M4)

Both land in the ONE `SCHEMA_VERSION = 4` bump so a corpus re-enriches exactly once (the coordination invariant from the
plan). At NAS scale (~2M images) the two together roughly halve the per-image disk and the resident search RAM.

**Decision (M3): embeddings are `f16`, not `f32`, on disk AND in the resident cache.** The CLIP (512-d) and Vision
feature-print (768-d) vectors are the biggest per-image storage item (5 KB of f32 → 2.5 KB of f16). `encode_embedding`
writes f16 le bytes; `decode_embedding` widens to f32 (the query direction — a find-similar source vector), while
`decode_embedding_f16` loads f16 as-is for the resident `BruteForceVectorStore`, so the cache is half the RAM too. Why
scoring runs against f16 directly rather than widening on load: `../vector/DETAILS.md`. Precision: f16 shifts a
realistic embedding's direction by cosine < 1e-3 (tested), far below ranking noise, and top-k order is preserved vs the
f32 reference (tested on a 100-vector fixture with 0.008 score gaps).

**Decision (M4): one `media_file(id, path)` identity table; every other table keys on `file_id`.** NAS paths average ~80
B and previously repeated in every table (`media_status`, `media_ocr`, `media_tags`, both embedding tables) — gigabytes
of pure duplication at 2M, plus string-compare joins. Now the path lives once in `media_file`; children carry the 8-byte
integer. **The Rust layer stays path-addressed** (the scheduler's `statuses` map, the read API's `ImageFacts`, the
vector store's `SimilarImage` all key on `String` paths): the store's reads join `media_file` back to a path, so nothing
above the store learned about ids. **Why not merge `media_status` into `media_file`:** they're 1:1, but keeping identity
(`media_file`) separate from enrichment-state (`media_status`) makes a rename a tiny one-row update and matches the
plan's explicit shape. **Rename** (`rename_path`) is the payoff the keying buys — a single `UPDATE media_file.path` and
every child follows via the unchanged `file_id`; it's the seam a future rename-following hook calls (until one is wired,
a rename still manifests as GC(old) + enrich(new), which this replaces with an O(1) update). ANN index keys are these
same `media_file` ids, so a rename needs no index touch at all (`../ann/DETAILS.md`).

The full read-path audit (every raw `path =` query against a media table became a `media_file` join): `read_status`,
`read_all_status`, `read_status_paths`, `sum_bytes_for_paths`, `read_all_embeddings_from`, `read_embedding_for`,
`read_tag_matches` (store); `search_ocr`, `facts_for_paths`, `images_with_tag` (read API); `scan_accounted` (coverage);
the writer's upsert / GC / prune / prune-prefix / purge.

## Testing

`tests.rs` covers the staleness key, the tags/embedding round-trip, tag-score filtering, the embedding codec (including
the f16 precision and top-k-order-preservation checks), and the clear-on-re-enrichment invariant, plus the FTS5
availability smoke. The writer-side prune primitives are tested in `../writer/tests.rs` (`../DETAILS.md` § Testing).
