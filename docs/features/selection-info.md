# Selection info

The selection info bar at the bottom of each file pane shows contextual information about the current selection or directory contents.

## Display modes

The info bar adapts its content based on view mode and selection state:

### Brief mode without selection

Shows details of the file under the cursor:
- Filename (middle-truncated if too long)
- Size in bytes with colored triads (or "DIR" for directories)
- Last modified date

### Full mode without selection

Shows directory totals:
```
No selection, 1,322 files and 23 dirs.
```

### With selection (both modes)

Shows selection summary:
```
450 892 of 1 492 837 (32%) selected in 2 of 1,322 files (1%) and 1 of 23 dirs (4%).
```

The byte count uses colored triads: bytes (gray), KB (blue), MB (green), GB (yellow), TB (red).

Hovering shows human-readable sizes: "440.13 KB of 1.42 MB"

### Empty directory

```
Nothing in here.
```

### Directories-only (no files)

When the directory contains only subdirectories, size cannot be shown:
```
3 of 23 dirs (13%) selected.
```

## Colored size triads

Byte sizes are split into three-digit groups from right to left, each colored by magnitude:

| Triad | Range | CSS class |
|-------|-------|-----------|
| Rightmost | 0–999 bytes | `size-bytes` |
| Second | 1–999 KB | `size-kb` |
| Third | 1–999 MB | `size-mb` |
| Fourth | 1–999 GB | `size-gb` |
| Fifth+ | 1+ TB | `size-tb` |

Example: `1 234 567` bytes displays as:
- "1" in `size-mb` (green)
- "234" in `size-kb` (blue)
- "567" in `size-bytes` (gray)

Triads are separated by thin spaces (U+2009).

## Implementation

### Backend (`operations.rs`)

The `get_listing_stats` function calculates totals and selection statistics:

```rust
pub struct ListingStats {
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_file_size: u64,
    pub selected_files: Option<usize>,
    pub selected_dirs: Option<usize>,
    pub selected_file_size: Option<u64>,
}
```

It iterates the cached listing once, summing totals and (if indices provided) selection counts.

### Frontend

**SelectionInfo.svelte** component with props:
- `viewMode: 'brief' | 'full'`
- `entry: FileEntry | null` - Entry under cursor (for Brief mode)
- `stats: ListingStats | null` - Directory and selection statistics
- `selectedCount: number` - Number of selected items

**FilePane.svelte** fetches stats:
1. On listing complete
2. When selection changes (tracked via `selectedIndices.size`)

**selection-info-utils.ts** exports:
- `formatSizeTriads(bytes)` - Split into colored triads
- `formatHumanReadable(bytes)` - "1.42 MB" format
- `pluralize(count, singular, plural)` - Grammar helper
- `formatNumber(n)` - Add thousand separators
- `calculatePercentage(part, total)` - Rounded percentage

## Testing

Tests in `selection-info-utils.test.ts` cover:
- Triad formatting for various byte sizes
- Human-readable formatting
- Percentage calculations
- Pluralization logic
