# Media store (`media.db`)

The per-volume SQLite cache: `mod.rs` (schema + reads), `connection.rs` (opening, `platform_case` registration).
`../writer/` owns every write, on ONE thread per volume. The disposable-cache and integer-id rules are in
`media_index/CLAUDE.md`.

## Must-knows

- **`SCHEMA_VERSION` is a cache version, not a migration target.** Bumping it delete-and-recreates every user's
  `media-{volume_id}.db` on next launch (Vision recompute, no re-download). That's the intended cost; ❌ don't write a
  migration.
- **`media_ocr` is a STANDALONE fts5 table, not external-content.** External content would sync via triggers off another
  table's integer rowid; the standalone table keyed by an UNINDEXED `file_id` keeps enrichment and GC a plain
  `WHERE file_id = ?` delete with no trigger machinery to desync. ❌ Don't "upgrade" it to external-content. It holds up
  to TWO rows per file, `source='ocr'` and `source='tag'`, so any read that means OCR text MUST filter on `source`.
- **`CREATE VIRTUAL TABLE … USING fts5` doubles as the FTS5 availability guard** — a build without FTS5 fails there,
  loudly, at creation rather than at query time.
- **An upsert is ONE transaction that clears every prior child row first**, so a re-enrichment can never leave a stale
  OCR/tag/embedding row behind; a failure clears them all and records only the `Failed` status. GC/prune delete every
  `file_id`-keyed child plus the `media_file` row.
- **`needs_enrichment` = `(path, mtime, size)` + the analyze stamp.** State is DELIBERATELY excluded, so a failed file
  isn't re-hammered every completed scan; a real file change re-tries it. CLIP staleness is a separate key (`needs_clip`
  over `clip_stamp`), decoupled on purpose.
- **`media_kind` and `state` are typed TEXT tokens parsed back to enums** (`sqlite3`-inspectable). ❌ Never branch on
  the raw string.
- **Embeddings are little-endian `f16` BLOBs.** `decode_embedding` widens to f32 for the query direction;
  `decode_embedding_f16` loads as-is for the resident cache. ❌ Don't widen on load — that forfeits the RAM halving
  (`../vector/CLAUDE.md`).

The table-by-table schema, the f16 + integer-id decisions, and the read-path audit: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
