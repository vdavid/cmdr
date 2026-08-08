# Units details

Pull-tier docs for `apps/desktop/src/lib/units/`. Must-knows are in `CLAUDE.md`.

## Why this module exists

A 764-file copy to an SMB share showed 83.65 MB in the copy dialog and 79.78 "MB" in the operation queue for the same
byte count, and "~8m 12s remaining" in one window against "5m 46s" in the other. Two separate causes, one shape: the
same quantity was turned into text in more than one place.

- The size gap was the reactive-settings layer never being initialized outside the main window (fixed in
  `settings/window-settings.ts`), compounded by four private byte formatters that hardcoded base 1024 while labelling
  the result "KB" / "MB" / "GB".
- The ETA gap was the copy dialog smoothing the backend's value while the queue row rendered it raw.

So: one implementation per quantity, and a lint that keeps it that way.

## The contract

- **A size** is `formatByteSize(bytes)` — friendliest unit, two fraction digits above the base, base and kilobyte casing
  from `appearance.fileSizeFormat`. `<Size bytes>` is the same output plus tier colors. A forced unit
  (`'kB'`/`'MB'`/`'GB'`) keeps a directory's sizes comparable; the file list's own unit mode routes through
  `formatSizeForDisplay`.
- **A speed** is the backend's `write-progress.bytesPerSecond` (`EtaEstimator` in
  `src-tauri/src/file_system/write_operations/eta.rs`), rendered as a `<Size>` inside the
  `fileOperations.shared.byteRate` catalog phrasing (`<size></size>/s`). The per-second marker is user-facing copy, so
  it belongs to the catalog, not to a code-side formatter; this module owns the number. The frontend does not compute a
  second, instantaneous rate for the active phase. The one frontend-computed rate is `ScanThroughput`, which covers the
  SCAN phase only, where the backend emits no rate at all.
- **An ETA** is the backend's `write-progress.etaSeconds` through `createEtaSmoother()`
  (`apps/desktop/src/lib/file-operations/progress-readout.ts`), then `formatDuration`. The smoother closes 25% of the
  gap per tick, so a real slowdown shows within about a second while single-tick jitter is damped. It's stateful, which
  is exactly why it is shared rather than reimplemented per window.

## Type safety: how far, and why not further

`ByteCount`, `BytesPerSecond`, and `Seconds` are zero-cost branded numbers. `Seconds` is **required** by
`formatDuration`, and all three are carried by `TransferReadout`
(`apps/desktop/src/lib/file-operations/progress-readout.ts`), which brands one `write-progress` payload at the IPC edge.
That's where the mistakes live: `bytesDone`, `filesDone`, `bytesPerSecond`, and `etaSeconds` sit next to each other as
bare numbers, and swapping two of them renders a plausible-looking wrong value rather than failing.

`formatByteSize` and `<Size bytes>` take a plain `number`. Re-examined 2026-08-01, after the index-crate extraction
landed, and the answer held for better reasons than the ones first recorded here:

- **A Rust `ByteSize` newtype buys nothing on the TypeScript side.** Measured against the pinned `specta` /
  `specta-typescript`: a derived newtype exports as `export type ByteSize = number`, which is structurally `number`, so
  `formatByteSize(entry.recursiveFileCount)` still compiles. A brand CAN be emitted, via `specta_typescript::define`,
  but only with a hand-written `Type` impl, a presentation dependency in `cmdr-fs`, and by downgrading `ByteCount`'s
  `unique symbol` key to a string literal any file can forge. That's a net loss.
- **The mistake it would catch barely exists.** In production Rust exactly one function takes a count and a byte count
  positionally where a swap compiles (`indexing::reconcile::local_reconcile::summary`, private, two correct callers).
  The progress cluster is already type-distinct: `files_done: usize` against `bytes_done: u64`.
- **The real risk is bytes-vs-bytes, which one newtype cannot separate.** `free_percent(total_bytes, available_bytes)`
  and the `-> (u64, u64)` returns (`get_live_storage_space`, `delete_counts`, …) swap into plausible wrong answers. The
  fix is a named struct, which is already the house pattern (`ExpectedTotals`, `VolumeSpaceInfo`), not a newtype.
- **The lint is blind to the argument, not to the call site.** `no-private-unit-format` polices WHERE formatting
  happens, never WHICH number is passed, so `formatByteSize(progress.filesDone)` passes it. Closing that means branding
  `formatByteSize`'s parameter (~58 sites, no Rust) — the one narrow slice worth doing if more safety is wanted.
  `<Size bytes>` is handed a RATE in three places, so its prop would need `ByteCount | BytesPerSecond`.

Two claims in the original version of this section were wrong and are corrected above: `bindings.ts` **can** carry a
brand, and the `FileEntry` blocker named `display_size` / `display_size_tooltip`, which are `Option<String>` git-portal
overrides ("12 commits ahead") and never byte counts at all. The byte fields are `size`, `physical_size`,
`recursive_size`, `recursive_physical_size`.

## The lint

`eslint-plugins/no-private-unit-format.js`, registered as `cmdr/no-private-unit-format` (error). Two detectors:

1. A **binary ladder literal** (`1024`, `1048576`, `1073741824`, `1099511627776`) as an operand of `*`, `/`, or `**`.
2. A **formatter-shaped name** (`formatBytes`, `formatSize`, `formatSpeed`, `formatEta`, …) whose body also contains a
   ladder literal or a unit label. The body condition is what lets a wrapper that delegates to `$lib/units` keep a
   descriptive name (`formatDbSize` in the debug panel does).

Base-1000 arithmetic is deliberately NOT flagged: `1000` is milliseconds-per-second far more often than it is a
kilobyte, and the AST can't tell. The name-plus-body detector covers SI formatters instead. Exempt by path:
`lib/units/`, `selection-info-utils.ts`, `intl/number-format.ts`, and test files. Off for `*.test.ts` in the config too,
since fixtures legitimately spell out `1024 * 1024`.

Rule tests: `eslint-plugins/no-private-unit-format.test.js`.

## What the consolidation replaced

Eight private implementations, all now routed through this module. Four were in the spec; the lint found four more:
`tauri-commands/write-operations.ts::formatBytes`, `query-ui/query-filter-state.svelte.ts` (parse + format),
`routes/debug/DebugDriveIndexPanel.svelte` (size + ms), `transfer/TransferConflictDialog.svelte` (threshold-based tier
class), `query-ui/recent-items/recent-items-utils.ts::formatBytes`, `settings/sections/AiLocalSection.svelte`
(`formatMemoryEstimate` + `formatMemoryGb`), `routes/debug/DebugSmbDiagnosticsPanel.svelte::fmtBytes`, and
`filter-chips/filter-popover-helpers.ts::kiloByteLabel` (now delegating).

Two behavior changes fall out, both intentional: the query-filter size bounds now use the user's base (the popover
already LABELLED them `kB` under SI while multiplying by 1024), and readouts that showed one fraction digit now show
two, matching the file list.
