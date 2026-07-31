# Media index read API

`MediaIndex` is the ONE consumer entry point for everything stored in `media.db` (`search/` included), modeled on
`importance`'s `ImportanceIndex`. It owns a `platform_case`-registered read connection and reads the DB DIRECTLY, so it
answers OFFLINE after a volume unmounts. ❌ Don't add a second reader elsewhere.

## Must-knows

- **Raw user input must NEVER reach `MATCH ?`.** fts5 parses the bound string as QUERY SYNTAX, so parens, colons, and a
  bareword `AND`/`OR` throw a syntax error and binding doesn't help. `build_ocr_match_query` quotes each whitespace
  token into a literal; every FTS entry point goes through it. (Same gotcha as `agent/store`'s `sanitize_fts_query`.)
- **Empty, never an error.** A missing / never-enriched / purged `media.db`, an empty query, or a feature-off volume
  answers with an empty result. Callers rely on this to voice coverage instead of surfacing a failure.
- **`media_ocr` holds up to two rows per file**, so any read that means OCR TEXT must filter `source = 'ocr'` — without
  it, tag labels come back dressed as recognized text. Structured tags come from `media_tags` instead, which keeps each
  label's own score.
- **`facts_for_paths` guarantees one row per requested path, in request order, keyed by the path AS REQUESTED.** A
  never-enriched file is representable (`indexed: false`), ❌ never silently dropped. It returns the FULL stored text,
  not a `snippet` (its caller is a model reasoning about the image, not a UI highlighting a match).
- **Chunk path lists at `PATH_CHUNK` (900) per `IN (…)`** — SQLite's default host-parameter ceiling is 999 and a rename
  over a big folder clears that immediately.
- **The reads are query-side only; they must not write.** Cache warming and invalidation belong to the pass seams
  (`../vector/CLAUDE.md`).

Each entry point's contract, the case-matching caveat, and the semantic-search route: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
