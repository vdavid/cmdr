# File explorer module

Dual-pane file explorer with keyboard-driven navigation, file selection, sorting, command palette, and adaptive layout.

## Selection (`selection/`)

### User interaction

- **Space** — toggle selection at cursor
- **Shift+click / Shift+arrow** — range selection with anchor (A) and end (B). If anchor already selected, range
  deselects.
- **Cmd+A / Cmd+Shift+A** — select all / deselect all
- **".." entry can't be selected**
- **Cleared on navigation** — selection is per-directory

### Implementation

- **State**: `Set<number>` in `FilePane.svelte` — O(1) add/remove/has
- **Preserved on sort/filter** — `resort_listing` accepts `selectedIndices[]`, returns `newSelectedIndices[]`
- **Write operations receive indices** — backend resolves to paths from cached listing
- **Visual**: `--color-selection-fg` (yellow foreground)

### Gotchas

- **Parent offset** — when `hasParent`, frontend indices = backend indices + 1
- **Range shrinking** — moving cursor back toward anchor removes items no longer in range
- **Optimization flag** — `allSelected: true` avoids sending 500k indices over IPC

## Navigation (`navigation/`)

### Back/forward navigation

- **Per-pane history** — independent stacks, session-only (not persisted)
- **Cross-volume** — works across local drives, network shares, MTP devices
- **Deleted folder handling** — walks up parent tree until existing dir found. Skipped entries remain in history.
- **Shortcuts**: Cmd+[ (back), Cmd+] (forward), Backspace (parent)

### Gotchas

- **History entries are paths** — stored as strings. No FileEntry metadata cached in history.
- **Volume switching** — changes volume context. History still tracks old volume's paths.

## Sorting

### Behavior

- **Directories first** — always
- **Natural sorting** — `file10.txt` after `file2.txt`
- **Extension grouping** — dotfiles → no-extension → by extension alphabetically
- **Per-column remembered order** — ascending/descending persisted in `settings.json`

### Implementation

- **Efficient re-sort** — `resort_listing` re-sorts cached listing without disk reads
- **Preserves cursor by filename** — frontend sends current filename, backend returns new index
- **Preserves selection by indices** — backend resolves filenames, re-finds after sort

### Gotchas

- **A and B cleared after sort** — range selection anchor/end reset (sorting is "new context")
- **Selected indices remapped** — backend returns `newSelectedIndices[]`, frontend updates `Set`

## Command palette (`command-palette/`)

### Features

- **Fuzzy search** — `@leeoniya/ufuzzy` (~3.5KB), typo-tolerant
- **Match highlighting** — matched characters underlined
- **Persisted query** — remembered within session
- **~45 commands** — all scopes (app, main window, file list, network, about)

### Gotchas

- **Low-level nav commands hidden** — `showInPalette: false` for arrow keys, Page Up/Down
- **Execution handler in +page.svelte** — `handleCommandExecute` delegates to `explorerRef`

## Font metrics (`font-metrics/`)

### What it does

Measures character widths for accurate column sizing in Brief mode. Avoids truncating filenames unnecessarily.

### Process

1. **First run** — measures ~67k chars (BMP + emoji) via Canvas API, ~100-300ms, background via `requestIdleCallback`
2. **Cache** — bincode2 binary, ~426KB, stored in `~/Library/Application Support/.../font-metrics/system-400-12.bin`,
   load time ~5ms
3. **Per-directory** — Rust sums char widths, returns `maxFilenameWidth`, BriefList uses for column width

### Gotchas

- **Fixed font for now** — system font, 400 weight, 12px. Hardcoded to match CSS `--font-system` at `--font-size-sm`.
- **Fallback** — if metrics unavailable, BriefList uses `containerWidth / 3`
- **Unmeasured chars** — rare Unicode falls back to average width
- **One font config at a time** — cache key includes font ID. When font becomes configurable, re-measure needed.
