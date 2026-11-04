# cmdr-brief-columns

Brief mode's column widths: `FontMetricsStore` (per-code-point widths the frontend measured with the Canvas API) and
`compute_brief_column_text_widths` (each column's widest name over the pane's rows). No `tauri`, no disk. Hosts: the app
(`apps/desktop/src-tauri/src/font_metrics/`, which persists the store) and the browser demo (`crates/cmdr-listing-wasm`,
memory only).

## Module map

- **metrics.rs**: `FontMetrics` (one size, and the app's on-disk format), `FontMetricsStore`, the scaled-size fallback,
  and the write wire (`widths_from_parallel`, `FontMetricsError`).
- **columns.rs**: `BriefColumnsQuery` in, `BriefColumnWidths` or the typed error out, and the tag-dot reservation.

## Must-knows

- **`average_width` is a one-paint stand-in, never a resting state.** An unmeasured code point is costed at the average
  AND reported in `missing_code_points`, so the frontend measures it and calls `extend_font_metrics`. ❌ Don't drop the
  `missing` accumulator to "simplify" a width call: every non-Latin filename's column would stay approximate forever.
- **The font ID is `"{family}-{weight}-{size}"`** and MUST match the frontend's `getCurrentFontId()`; a mismatch is
  silent (`FontMetricsNotReady`, or a borrowed size). An unmeasured size borrows the nearest stored size of its family
  and weight, scaled, and answers `approximate: true`.
- **`FontMetrics`' fields are the desktop's file format** (bincode2). ❌ Never reorder, rename, or retype one; the app's
  `the_disk_format_is_pinned` test pins the bytes.
- **`with_rows`' closure runs under the listing cache's lock**: it measures and returns. ❌ Never call back into the
  engine from inside it.
- **Same wire on both hosts.** Errors cross as `BriefColumnsIpcError { kind, message }`; the frontend branches on `kind`
  alone, ❌ never on `message`. `tag_cluster_width` mirrors `tagClusterWidthPx` in `tag-dots-utils.ts`.
- **Browser-safe**: the clock is `cmdr_fs::rt` (`std::time::Instant::now` panics in wasm). The surface is capped by
  `index-crate-isolation`.

Decisions (Canvas over Rust fonts, widths in Rust, the store as a value, the fill-in contract) and the capped surface:
`DETAILS.md`.
