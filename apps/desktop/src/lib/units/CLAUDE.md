# Units

The one place a byte count, a transfer rate, or a duration becomes text a person reads. Start here before writing any
size, speed, or ETA to the DOM.

```ts
import { formatByteSize, formatDuration, seconds } from '$lib/units'

formatByteSize(87_654_321) // "83.59 MB" (binary) or "87.65 MB" (SI)
formatDuration(seconds(492)) // "8m 12s"
```

A transfer RATE is a size plus a per-second marker; that marker is user-facing copy, so render
`<Trans key="fileOperations.shared.byteRate" snippets={{ size }} />` over a `<Size bytes={rate}>` snippet (what the copy
dialog and the operation queue both do) rather than adding a code-side rate formatter.

## Module map

- `index.ts`: the public surface. `formatByteSize` reads the user's `appearance.fileSizeFormat`; everything else
  re-exports from the two leaves.
- `byte-size.ts`: the unit math with the base passed in (`formatFileSizeWithFormat`, `unitLabel`, `fixedUnitFor`,
  `dynamicTierIndex`, `baseFor`), plus the `ByteCount` / `BytesPerSecond` brands.
- `duration.ts`: `formatDuration` (seconds), `formatMilliseconds` (sub-second precision), `formatFilesPerSecond`, and
  the `Seconds` brand.

## Must-knows

- **❌ Never write a private `formatBytes` / `formatSpeed` / `formatEta`.** Four once drifted apart here, each
  hardcoding base 1024 while labelling the result "KB"/"MB"/"GB", which is how two windows came to show different
  numbers for the same transfer. `cmdr/no-private-unit-format` rejects new ones (binary ladder literals in arithmetic,
  and formatter-shaped names whose body does unit work). Opt out per-line with a reason for a genuine fixed binary
  threshold.
- **`<Size bytes>`** (`$lib/ui/Size.svelte`) is the COMPONENT form: same numbers plus the size-tier colors. Prefer it in
  markup; use `formatByteSize` for tooltips, toasts, and anything composing a string. `<Size bytes rounded>` drops the
  decimals ("7 GB") — for a LIVE readout only (the transfer bars), where the number changes several times a second; a
  size someone compares or copies keeps them.
- **`formatDuration` requires a branded `seconds(n)`**, and rates are branded `bytesPerSecond(n)`. IPC hands you bare
  numbers, so brand at the edge — for `write-progress` that's `transferReadout(event)` in
  `apps/desktop/src/lib/file-operations/progress-readout.ts`. `formatByteSize` takes a plain `number` on purpose: ~40
  call sites, and the lint rather than the type is what guards it.
- **Size-tier COLORING is a separate layer**: `formatSizeForDisplay` / `colorizeSizeString` / `sizeTierClasses` in
  `file-explorer/selection/selection-info-utils.ts`, because the classes belong to the list views' stylesheet. It
  consumes this module's ladder; don't re-derive tiers from a threshold cascade.
- **Dates are NOT here.** `settings/format-utils.ts` (pure) → `formattedDate()` (reactive) → `<DateLabel>`.
- **Decimals and separators follow the active locale** via `$lib/intl`'s `getNumberFormatter`; the value↔unit ASCII
  space is added by us, never by Intl (`colorizeSizeString` parses the unit by the last space).
- **`settings/types.ts::formatDurationSetting(ms)` is a deliberate second duration formatter** for rendering a duration
  SETTING's stored value in the settings UI ("500ms" / "5min"). Different surface, different shape; don't merge them.

Rationale, the speed/ETA definitions, and the type-safety decision: `DETAILS.md`.
