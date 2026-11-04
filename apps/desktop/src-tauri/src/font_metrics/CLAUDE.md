# Font metrics

The desktop's host for `cmdr-brief-columns` (the width math, the store, and the Brief column computation:
`crates/cmdr-brief-columns/CLAUDE.md`). This module holds only what needs the app: the process-wide
`FontMetricsStore` (`store()`) and its on-disk cache, one bincode2 file per font ID.

- `mod.rs`: `store_and_persist`, `extend_and_persist`, `has_metrics`, and the startup loads (`init_font_metrics`,
  `load_all_metrics_from_disk`, which pre-loads every `*.bin` so a customized text size is warm on first paint).
- `brief_columns.rs`: `compute_brief_column_text_widths` over `listings()` and `store()`, plus the E2E failure hook
  (`FAIL_NEXT_WIDTH_CALLS`, `playwright-e2e` only). The commands: `commands/font_metrics.rs` and
  `get_brief_column_text_widths` in `commands/file_system/listing.rs`.

## Must-knows

- **Both writes serialize under the store's lock, then write the file after releasing it** (`extend_with` runs the
  serializer). That's what lets an extend merge into the stored map without cloning it out.
- **The file is `FontMetrics`' serde form**, pinned byte for byte by `the_disk_format_is_pinned`. A file that doesn't
  decode is skipped silently and the size re-measured, so a format change would cost every user a re-measure, never an
  error.
- **The E2E hook fires only after the `items_per_column == 0` rejection would have**, as it did inside the computation.

Disk format and why binary: `DETAILS.md`.
