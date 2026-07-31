# Units details

Pull-tier docs for `apps/desktop/src/lib/units/`. Must-knows are in `CLAUDE.md`.

## Why this module exists

A 764-file copy to an SMB share showed 83.65 MB in the copy dialog and 79.78 "MB" in the Transfers window for the same
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

`formatByteSize` and `<Size bytes>` deliberately take a plain `number`:

- ~40 call sites would each need an `bytes(...)` wrapper, and the generated `lib/ipc/bindings.ts` can't carry a brand,
  so every one of them would be branding a number it just received unbranded. That's ceremony, not proof.
- The Rust half of a whole-app byte-count newtype is not ours to design right now: `FileEntry` in `crates/cmdr-fs`
  carries `display_size` and `display_size_tooltip`, and that crate is being reshaped by the index-crate extraction.
  Adding a `ByteSize` newtype crossing IPC would mean redesigning a type that effort owns while it owns it, and
  regenerating `bindings.ts` into its path.
- `cmdr/no-private-unit-format` is what actually prevents divergence, and it covers the whole app including markup.

If the Rust newtype is ever wanted, do it after the extraction lands, and start from `FileEntry`.

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
