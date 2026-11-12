# Media index read API — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.
The IPC commands that wrap these entry points live in `apps/desktop/src-tauri/src/commands/media_index/search.rs`
(`../DETAILS.md` § The IPC surface).

## The entry points

`MediaIndex` (media_index Decision 8) opens `media-{volume_id}.db` and answers:

- `search_ocr(query, limit)` → `OcrHit`s (path + a highlighted `snippet`, the "why matched" reason).
- `facts_for_paths(&[&str])` → `Vec<ImageFacts>` (§ The lookup direction).
- `images_with_tag(label, min_score)` → `TagHit`s, the structured tag-score filter over `media_tags`.
- `find_similar(source_path, k)` → the source embedding's `top_k` over the resident feature-print cache, source
  excluded; `dedup_clusters(threshold)` → near-duplicate clusters.
- `search_semantic(query_vec, k)` → CLIP top-k. It takes the ALREADY-ENCODED query vector; the command owns the
  tokenize + encode, and why the seam sits there is `../clip/DETAILS.md` § The query path. It routes between the exact
  resident scan and the ANN index by corpus size (`../ann/DETAILS.md`).
- `enriched_count()` → the `COUNT(*)` behind the per-volume coverage signal.

## The lookup direction (`facts_for_paths`)

Every other read is query-direction (a query in, matching paths out). `facts_for_paths` is the opposite: the caller
already has the paths (the user navigated to a folder) and asks what's stored for each. It backs the `image_facts` MCP
tool and the natural-language bulk-rename flow that needs to know what's IN an image before proposing a name. Four
properties it exists to guarantee:

- **The FULL stored text, not a snippet.** `search_ocr` returns `snippet(media_ocr, 2, …)` because a UI highlights a
  match; a model naming a file has to read the whole thing.
- **OCR text and tags stay DISTINCT.** `media_ocr` holds up to two rows per file behind an UNINDEXED `source` column
  (`'ocr'` = recognized text, `'tag'` = the space-joined tag labels folded in for keyword search), so the text read
  filters `source = 'ocr'`; without that filter the tag labels come back dressed as recognized text. Tags are read from
  the STRUCTURED `media_tags` table instead, so each keeps its own label and score rather than the folded, score-less
  FTS row.
- **One row per requested path, in request order, keyed by the path AS REQUESTED.** A never-enriched file is
  representable (`indexed: false`), never silently dropped, so a caller can tell "ask again once indexing catches up"
  from "indexed, and there's genuinely no text in it" (`indexed: true`, `ocr_text: None`). A missing `media.db` answers
  every path as not-indexed rather than erroring, keeping the module's empty-not-error convention while still honoring
  the one-row-per-path contract.
- **Chunked at 900 paths per `IN (…)`.** SQLite's default host-parameter ceiling is 999 and a rename over a big folder
  clears that immediately.

**Case-matching caveat.** All three queries join `media_file`, whose `path` column carries `COLLATE platform_case`, so
SQLite matches case-insensitively on a case-insensitive volume — but the returned row is then mapped back to the request
slot through an EXACT-string `by_path` map in Rust. A caller passing a differently-cased spelling than the indexer
stored therefore reads as not-indexed. Callers pass paths from the same index/UI the enrichment pass saw, so this
doesn't bite in practice; don't "fix" it by lowercasing, which would break case-sensitive volumes.

## Excluded folders at read time (`exclusion.rs`)

Excluding a folder vetoes its enrichment and purges its rows, but no read leans on either: a purge can fail to land
(`../scheduler/purge.rs` keeps retrying it), a folder excluded while its NAS was offline is only purged on reconnect,
and a row can commit between the veto and the delete. Ask Cmdr's `search_photos` and `image_facts` tools read through
`MediaIndex`, so this filter is what keeps an excluded folder's OCR text away from a cloud model.

- **One predicate, the veto's.** `ReadExclusion::hides` joins the stored path onto the volume's roots and asks
  `NetworkEnrichConfig::is_excluded`, the same component-boundary prefix match (`network/config.rs::path_is_within`) the
  enrichment veto and the purge use, so reading can't disagree with indexing about what's excluded. It folds names the
  way the drive index's `platform_case` collation does (`normalize_for_comparison`): on macOS an NFC exclusion covers
  NFD paths and a case-only difference matches; elsewhere it's byte-exact.
- **Where a volume's rows sit: every root it's known by.** Each local and network pass records its mount root in `meta`
  through the writer (`mount_root:<root>`, one row per root, never overwritten), and the live mount root joins them
  while the volume is mounted. A row is hidden when a folder excludes it at ANY of them, so an unmounted NAS still hides
  its excluded folders, and a share remounted as `/Volumes/testshare-1` keeps a folder excluded at
  `/Volumes/testshare/Photos` hidden. The network pass's veto (`is_excluded_at_known_roots`) and the retro-delete take
  the same union. A volume with no known root shows NOTHING while any folder is excluded, until a pass records one; only
  a NAS indexed before roots were recorded, and offline ever since, can land there.
- **Filter before the cut.** A ranked read that dropped hidden rows after truncating would answer fewer than `k` hits
  whenever the excluded images ranked first. So the vector scans skip inside `top_k`; dedup drops skipped images BEFORE
  clustering (single linkage would otherwise chain two visible images through a hidden one and give it away); the OCR
  search re-runs with a 4× larger window while hidden rows leave it short and matches remain; and the ANN over-fetch
  falls back to the exact scan when hiding thinned it below `k`. With no folder excluded, all of it costs one config
  snapshot per read.
- **Per path.** `facts_for_paths` answers an excluded path as never indexed (its path is never bound into a query),
  `status_for_paths` omits it (the file badge then reads `excluded`), and `find_similar` gives an excluded source no
  neighbors. `enriched_count` still counts every stored row: a number, never an image.

## Testing

`tests.rs` covers the OCR search round-trip, the fts5 sanitizer (`build_ocr_match_query`, pure — hostile query syntax
must not error), the empty-not-error paths, and offline-after-unmount (enrich over the fake backend, drop the writer,
assert the search still answers). Its § "Excluded folders never surface" asks each read path once about an image under
an excluded folder, plus an offline NAS placed by its recorded root and a volume with no known root. The vector-math
side of `find_similar` / `dedup_clusters` / `search_semantic` is tested in `../vector/tests.rs` and `../ann/tests.rs`
(which also pins the ANN fallback when every over-fetched candidate is excluded).
