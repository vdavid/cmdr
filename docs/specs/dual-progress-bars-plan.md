# Dual progress bars for file operations

## Why

The `TransferProgressDialog` shows one byte-based progress bar with text stats below. But file operations have two
independent progress dimensions: bytes transferred and files processed. These can diverge significantly — copying one
huge file vs many tiny files produces very different bar behaviors. Showing both gives the user a clearer mental model
of what's happening (design principle: radical transparency).

The progress bar code is also duplicated — `TransferProgressDialog` and `ProgressOverlay` each have their own inline CSS
bars with no shared component.

## What we're building

1. A reusable `ProgressBar.svelte` component (pure visual bar, no layout opinions)
2. Dual progress bars in `TransferProgressDialog` (size + file count)
3. `ProgressOverlay` refactored to use the shared component

## What we're NOT building

**Rollback progress.** The backend's `CopyTransaction::rollback()` (`state.rs:381`) is synchronous with no event
emission. Backwards-animating bars would require backend changes (emitting progress during file deletion). The current
spinner + "Deleting N copied files..." is correct for an indeterminate operation. Future work if needed.

## Design decisions

### ProgressBar is just the bar

**Why:** `ProgressOverlay` uses a horizontal row layout (bar + percentage + ETA inline). `TransferProgressDialog` needs a
grid layout (label, bar, detail). These are fundamentally different layouts. If the component includes label/detail props,
one consumer won't use them, making the API dishonest. A pure bar component is more composable — consumers arrange labels
however they need.

### Size variants instead of pixel props

**Why:** The codebase uses design tokens (`--spacing-*`, `--radius-*`, `--font-size-*`) rather than raw values. A
`size: 'sm' | 'md'` prop (4px / 8px) is idiomatic. `sm` for `ProgressOverlay`'s compact floating indicator, `md` for
dialog-level progress. Border radius scales with size: `sm` uses `--radius-xs` (2px), `md` uses `--radius-sm` (4px) —
matching what each consumer had before.

### No fillColor prop

The only potential use was rollback styling, which we're not building. Removing it keeps the API minimal and avoids
an escape hatch that bypasses the design token system. Can be added later if a real use case arises.

### Inline grid layout for dual bars

**Why:** A vertical stack of label-above-bar pairs creates a text/bar/text/bar/text sandwich — visually noisy. An inline
grid aligns both bars side by side with labels left and stats right. Compact, scannable, bars are the focal element:

```
Size   [========================] 1.2 / 3.5 GB (34%)
Files  [==================      ] 42 / 100
       45.2 MB/s · ~2 min remaining
```

CSS grid with `grid-template-columns: auto 1fr auto`. Meta row spans all columns.

### Hide progress grid during scanning phase

During scanning, both `bytesTotal` and `filesTotal` are 0. Two empty bars at 0% with "0 / 0" labels is ugly and
uninformative. The stage indicator already communicates "Scanning" with a spinner. The progress grid is only shown once
the active phase begins (when `phase !== 'scanning'`).

### Hide size bar when byte totals are unavailable

Trash operations without `itemSizes` (when item sizes aren't known) have `bytesTotal = 0` throughout. A size bar stuck
at "0 B / 0 B (0%)" is misleading. Conditionally hide the size row when `bytesTotal === 0` and we're past scanning.

## Implementation

### Step 1: Create `ProgressBar.svelte`

**File:** `apps/desktop/src/lib/ui/ProgressBar.svelte`

Props:
- `value: number` — 0–1 fractional progress (required)
- `size?: 'sm' | 'md'` — bar height + radius. `sm` = 4px / `--radius-xs`, `md` = 8px / `--radius-sm`. Default `'md'`
- `ariaLabel?: string` — accessible label for screen readers

Internals:
- `percent = $derived(Math.min(100, Math.round(value * 100)))`
- Track div: `role="progressbar"`, `aria-valuenow={percent}`, `aria-valuemin={0}`, `aria-valuemax={100}`,
  optional `aria-label={ariaLabel}`
- Fill div: `transition: width 0.15s ease-out` (standardized from the existing 0.1s and 0.3s)
- Styling: `--color-bg-tertiary` track, `--color-accent` fill

Note: This also improves accessibility — the existing `TransferProgressDialog` bar had no `role="progressbar"` or ARIA
attributes.

### Step 2: Refactor `ProgressOverlay.svelte`

**File:** `apps/desktop/src/lib/ui/ProgressOverlay.svelte`

**Intent:** Validate the new component in the simpler consumer first.

- Import and use `<ProgressBar value={progress ?? 0} size="sm" />`
- Remove the inline `.progress-bar` and `.progress-fill` CSS rules
- Keep `.progress-row`, `.progress-text`, `.progress-eta` (the surrounding layout is unchanged)

### Step 3: Dual bars in `TransferProgressDialog.svelte`

**File:** `apps/desktop/src/lib/file-operations/transfer/TransferProgressDialog.svelte`

Script:
- Import `ProgressBar`
- Remove `percentComplete` derived (line 189) — no longer used, replaced by inline expressions

Template — replace lines 799–828 (progress section + stats section) with a CSS grid. Preserve the current file
indicator (lines 830–835) after the grid — it should remain outside the `{#if}` since the backend emits `current_file`
during scanning too, and showing it supports radical transparency. Only show the bars when past scanning:

```svelte
{#if phase !== 'scanning'}
    <div class="progress-grid">
        {#if bytesTotal > 0}
            <span class="progress-label">Size</span>
            <ProgressBar value={bytesDone / bytesTotal} ariaLabel="Size progress" />
            <span class="progress-detail">
                {formatBytes(bytesDone)} / {formatBytes(bytesTotal)}
                ({Math.round((bytesDone / bytesTotal) * 100)}%)
            </span>
        {/if}

        <span class="progress-label">{operationType === 'trash' ? 'Items' : 'Files'}</span>
        <ProgressBar value={filesTotal > 0 ? filesDone / filesTotal : 0} ariaLabel="File progress" />
        <span class="progress-detail">{filesDone} / {filesTotal}</span>

        <div class="progress-meta">
            {#if stats.bytesPerSecond > 0}
                <span class="progress-speed">{formatBytes(stats.bytesPerSecond)}/s</span>
            {/if}
            {#if stats.estimatedSecondsRemaining !== null}
                <span class="progress-eta">~{formatDuration(stats.estimatedSecondsRemaining)} remaining</span>
            {/if}
        </div>
    </div>
{/if}
```

CSS:
- `.progress-grid`: `display: grid; grid-template-columns: auto 1fr auto; gap: var(--spacing-xs) var(--spacing-sm);
  align-items: center; padding: 0 var(--spacing-xl); margin-bottom: var(--spacing-md)`
- `.progress-label`: `font-size: var(--font-size-sm); color: var(--color-text-tertiary)`
- `.progress-detail`: `font-size: var(--font-size-sm); color: var(--color-text-secondary);
  font-variant-numeric: tabular-nums; text-align: right`
- `.progress-meta`: `grid-column: 1 / -1; display: flex; justify-content: space-between;
  font-size: var(--font-size-sm)`
- `.progress-speed`: `color: var(--color-text-secondary); font-variant-numeric: tabular-nums`
- `.progress-eta`: `color: var(--color-text-tertiary)`
- Remove old: `.progress-section`, `.progress-bar-container`, `.progress-bar`, `.progress-info`, `.progress-percent`,
  `.eta`, `.stats-section`, `.stat-row`, `.stat-label`, `.stat-value`

### Step 4: Update docs

**`apps/desktop/src/lib/ui/CLAUDE.md`** — add ProgressBar section following the existing format (see ProgressOverlay
section for the pattern). Props table with types and notes. Mention consumers (ProgressOverlay, TransferProgressDialog).

**`apps/desktop/src/lib/file-operations/CLAUDE.md`** — in the TransferProgressDialog bullet, update
"Progress bar with ETA, speed (MB/s), current file" → "Dual progress bars (size + file count) with speed, ETA, current
file"

### Step 5: Checks

- `./scripts/check.sh --svelte` (typecheck, lint, stylelint, format)

## Testing

- Manual: `pnpm dev` → copy a large folder → verify two bars animate independently
- Manual: delete files → both bars progress (backend tracks bytes for delete/trash too)
- Manual: trash files → "Items" label appears instead of "Files"
- Manual: cancel a copy → rollback spinner still works unchanged
- Manual: trigger indexing → ProgressOverlay bar still works
- Manual: start an operation → verify scanning phase shows stages but no empty bars
- Automated: existing vitest + checks should pass (no behavior changes, just UI restructuring)
