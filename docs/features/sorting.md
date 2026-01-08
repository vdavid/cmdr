# File sorting

Cmdr supports sorting files by column headers in the file list.

## User interaction

Click any column header to sort by that column:

- **Name**: Alphabetical, with natural sorting (e.g., `img_2` before `img_10`)
- **Size**: File size in bytes
- **Modified**: Last modification date

Clicking the same column toggles between ascending (▲) and descending (▼) order.

## Sorting behavior

### Directories first

Directories always appear before files, regardless of sort column.

### Natural sorting

Names are sorted alphanumerically, so `file10.txt` comes after `file2.txt`, not before.

### Extension sorting

When sorting by name, files without extensions are grouped logically:

1. Dotfiles (e.g., `.gitignore`)
2. Files without extension
3. Files by extension alphabetically

### Per-pane state

Each pane remembers its own sort column independently.

### Remembered sort orders

Sort order (ascending/descending) is remembered per-column across app restarts.

## Implementation

### Backend (Rust)

- `SortColumn` enum: `name`, `extension`, `size`, `modified`, `created`
- `SortOrder` enum: `ascending`, `descending`
- `sort_entries()` function with multi-key sorting
- `resort_listing()` command for efficient in-place re-sorting without disk reads

### Frontend (Svelte)

- `SortableHeader.svelte` - Reusable clickable column header component
- `FullList.svelte` - Header row with sortable Name, Size, Modified columns
- `DualPaneExplorer.svelte` - Manages per-pane sort state and persistence
