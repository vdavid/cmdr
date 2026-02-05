# Agent-centric MCP redesign

## Problem

The current MCP server design is **UI-centric** (mirrors keyboard shortcuts) rather than **agent-centric** (what an
agent actually needs to accomplish tasks efficiently).

An agent using this MCP today would need to:

1. Make 6+ resource calls just to understand the current state
2. Execute dozens of `nav_down` calls to find a file
3. Guess indices for volume/file selection without context

## Plan

No need to be backwards compatible.

### 1. Consolidate resources into one "state" endpoint

**Remove**: `cmdr://pane/focused`, `cmdr://pane/left/path`, `cmdr://pane/right/path`, `cmdr://pane/left/content`,
`cmdr://pane/right/content`, `cmdr://pane/cursor`, `cmdr://status`, `cmdr://volumes`, `cmdr://selection`

**Add**: One `cmdr://state` resource that returns the complete app state:

```yaml
focused: left
showHidden: false

left:
  volume: Macintosh HD
  path: /Users/david/projects
  view: full
  sort: name:asc
  totalFiles: 50000
  loadedRange: [49500, 50000]  # What's currently loaded in viewport
  cursor:
    index: 49503  # Absolute index. No extra info because it's full mode (info is in files list)
  selected: 18  # Count of selected files
  files:
    - i:49500 d .git
    - i:49501 d src [sel]
    - i:49502 f README.md 2403b cr:2025-01-15 lm:2025-01-15  # Full info because full mode
    - i:49503 f package.json 1183b cr:2025-01-10 lm:2025-01-10 [cur]
    # ... up to 500 files (what's loaded)

right:
  volume: External SSD
  path: /Volumes/External/backup
  view: brief
  sort: modified:desc
  totalFiles: 50
  loadedRange: [0, 50]
  cursor:  # Extra info only on cursor file because it's brief mode
    index: 0
    name: 2025-01
    size: 4096
    created: 2025-01-01
    lastModified: 2025-01-15
  selected: 0
  files:
    - i:0 d 2025-01 [cur]  # No extra info because brief mode
    - i:1 d 2024-12

volumes:
  - Macintosh HD
  - External SSD

dialogs:
  - type: settings
    section: general
    focused: true
  - type: file-viewer
    path: /Users/david/readme.md
  - type: file-viewer
    path: /Users/david/package.json
```

**Design choices:**

- **YAML** over JSON — more readable, ~30-40% smaller
- **Inline markers**: `[cur]` for cursor, `[sel]` for selected
- **Compact file format**: index, type (d/f/l), name, size, dates
- **View-mode-aware detail**: Full mode shows all file info; brief mode shows info only for cursor file
- **Pagination**: `totalFiles` + `loadedRange` for large directories. Files list contains what's loaded in viewport.
- **Dialogs as list**: Each has `type`, type-specific params, and optional `focused: true`
- **Closed dialogs omitted**: Less noise — if it's not in the list, it's not open

**Optional params** (TBD): `?limit=100`, `?pane=left`

### 2. Available dialogs resource

Separate from state (it's static metadata):

```yaml
# cmdr://dialogs/available
- type: settings
  sections: [general, appearance, shortcuts, advanced]
- type: volume-picker
  description: Only one can be open at a time
- type: file-viewer
  description: Opens for file under cursor, or specify path. Multiple can be open.
- type: about
- type: confirmation
  description: Copy/mkdir confirmation. Opened by copy/mkdir tools, not directly. Can only be closed.
```

### 3. Replace granular navigation with semantic tools

**Remove**: `nav_up`, `nav_down`, `nav_left`, `nav_right`, `nav_home`, `nav_end`, `nav_pageUp`, `nav_pageDown`

**Add:**

| Tool | Description | Example |
|------|-------------|---------|
| `select_volume` | Switch pane to volume | `{pane: "left", name: "External SSD"}` |
| `nav_to_path` | Navigate pane to path | `{pane: "left", path: "/Users/david"}` |
| `move_cursor` | Move cursor to index or name | `{pane: "left", to: 5}` or `{pane: "left", to: "package.json"}` |
| `scroll_to` | Load region for large directories | `{pane: "left", index: 25000}` |

**Keep (renamed):**

| Old | New | Description |
|-----|-----|-------------|
| `nav_open` | `open_under_cursor` | Enter folder or open file |
| `nav_parent` | `nav_to_parent` | Go up one level |
| `nav_back` | `nav_back` | History back |
| `nav_forward` | `nav_forward` | History forward |

### 4. One selection tool

**Remove**: `selection_clear`, `selection_selectAll`, `selection_deselectAll`, `selection_toggleAtCursor`,
`selection_selectRange`

**Add one tool:**

```
select: {
  pane: "left" | "right",
  start: <number>,              # 0-indexed, required
  count: <number> | "all",      # Items from start. 0 = clear selection
  mode: "replace" | "add" | "subtract"  # Optional, default: "replace"
}
```

**Examples:**

| Goal | Call |
|------|------|
| Select indices 5-10 | `{pane: "left", start: 5, count: 6}` |
| Select all | `{pane: "left", start: 0, count: "all"}` |
| Clear selection | `{pane: "left", start: 0, count: 0}` |
| Add 5 more starting at 20 | `{pane: "left", start: 20, count: 5, mode: "add"}` |
| Deselect indices 3-5 | `{pane: "left", start: 3, count: 3, mode: "subtract"}` |

### 5. One dialog tool

```
dialog: {
  action: "open" | "focus" | "close",
  type: "settings" | "volume-picker" | "file-viewer" | "about" | "confirmation",
  # Type-specific params:
  section?: "...",  # For settings: which section to open
  path?: "...",     # For file-viewer: which file (required for focus/close if multiple open)
}
```

**Behavior notes:**

- `file-viewer` without `path` on open → opens for file under cursor
- `file-viewer` without `path` on focus → focuses most recently opened file-viewer
- `file-viewer` without `path` on close → closes all file-viewer dialogs
- `confirmation` → the copy/mkdir confirmation dialog; only `close` action is useful (to cancel)

**Examples:**

- Open settings: `{action: "open", type: "settings", section: "shortcuts"}`
- Open file viewer for cursor: `{action: "open", type: "file-viewer"}`
- Open file viewer for specific file: `{action: "open", type: "file-viewer", path: "/Users/david/readme.md"}`
- Focus specific file viewer: `{action: "focus", type: "file-viewer", path: "/Users/david/readme.md"}`
- Close specific file viewer: `{action: "close", type: "file-viewer", path: "/Users/david/readme.md"}`
- Close all file viewers: `{action: "close", type: "file-viewer"}`
- Cancel copy/mkdir: `{action: "close", type: "confirmation"}`

**Return values:**

- `OK: Opened settings`
- `OK: Opened file viewer for /Users/david/readme.md`
- `OK: Closed 2 file viewer dialogs`
- `OK: Cancelled confirmation dialog`

### 6. Consolidate sort tools

**Remove**: `sort_byName`, `sort_byExtension`, `sort_bySize`, `sort_byModified`, `sort_byCreated`, `sort_ascending`,
`sort_descending`, `sort_toggleOrder`

**Add one:**

```
sort: {pane: "left", by: "name" | "ext" | "size" | "modified" | "created", order: "asc" | "desc"}
```

### 7. File operation tools

These trigger native dialogs and return immediately. **Agent must wait for user confirmation.**

| Tool | Description | Notes |
|------|-------------|-------|
| `copy` | Copy selected files to other pane | Opens confirmation dialog (F5). Requires user approval. Don't use if user is away. |
| `mkdir` | Create folder in focused pane | Opens naming dialog (F7). Requires user input. Don't use if user is away. |
| `refresh` | Refresh focused pane | No dialog needed |

**Return value:** `OK: Copy dialog opened. Waiting for user confirmation.`

If the user doesn't confirm it, the agent can close the confirmation dialog like any dialog.

Note: move/delete/rename features don't exist in the app yet.

### 8. Other tools

| Tool | Description | Example |
|------|-------------|---------|
| `switch_pane` | Toggle focus between panes | (no params) |
| `toggle_hidden` | Toggle hidden files visibility | (no params) |
| `set_view_mode` | Set view mode | `{pane: "left", mode: "brief" \| "full"}` |
| `quit` | Quit application | (no params) |

### 9. Response format

Use plain text for tool results:

```
OK: Cursor moved to index 5 (package.json)
OK: Navigated to /Users/david/projects
OK: Selected 15 files
OK: Copy dialog opened. Waiting for user confirmation.
```

For errors:

```
ERROR: Path not found: /Users/david/nonexistent
ERROR: Index 500 out of range (max: 127)
ERROR: No file-viewer open for /Users/david/unknown.md
ERROR: No confirmation dialog open
```

## Summary: Tools list

**Navigation (6)**

- `select_volume`, `nav_to_path`, `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`

**Cursor & selection (3)**

- `move_cursor`, `open_under_cursor`, `select`

**File operations (3)**

- `copy`, `mkdir`, `refresh`

**View (3)**

- `sort`, `toggle_hidden`, `set_view_mode`

**Dialogs (1)**

- `dialog`

**App (2)**

- `switch_pane`, `quit`

**Total: 18 tools** (down from 51)

## Summary: Resources list

- `cmdr://state` — complete app state
- `cmdr://dialogs/available` — what dialogs can be opened

**Total: 2 resources** (down from 9)

---

## Task list

### Milestone 1: New resources ✅

- [x] Implement `cmdr://state` resource with YAML output
- [x] Include both panes: volume, path, view, sort, totalFiles, loadedRange, cursor, selected, files
- [x] Implement compact file format with inline markers (`[cur]`, `[sel]`)
- [x] Implement view-mode-aware detail (full mode: all file info; brief mode: cursor file only)
- [x] Implement `cmdr://dialogs/available` resource
- [x] Add pagination support (totalFiles, loadedRange)
- [x] Remove old resources: `cmdr://pane/*`, `cmdr://status`, `cmdr://volumes`, `cmdr://selection`

### Milestone 2: Navigation tools ✅

- [x] Implement `select_volume` tool
- [x] Implement `nav_to_path` tool
- [x] Implement `move_cursor` tool (accepts index or filename)
- [x] Implement `scroll_to` tool for large directories
- [x] Rename `nav_open` → `open_under_cursor`
- [x] Rename `nav_parent` → `nav_to_parent`
- [x] Keep `nav_back`, `nav_forward` as-is
- [x] Remove old navigation tools: `nav_up`, `nav_down`, `nav_left`, `nav_right`, `nav_home`, `nav_end`, `nav_pageUp`, `nav_pageDown`

### Milestone 3: Selection tool ✅

- [x] Implement unified `select` tool with `start`, `count`, `mode` params
- [x] Support `count: "all"` for select-all
- [x] Support `count: 0` for clear selection
- [x] Support `mode: "add" | "subtract" | "replace"`
- [x] Remove old selection tools: `selection_clear`, `selection_selectAll`, `selection_deselectAll`, `selection_toggleAtCursor`, `selection_selectRange`

### Milestone 4: Dialog tool ✅

- [x] Implement unified `dialog` tool with `action`, `type`, `section`, `path` params
- [x] Support `action: "open" | "focus" | "close"`
- [x] Support `type: "settings" | "volume-picker" | "file-viewer" | "about" | "confirmation"`
- [x] Implement file-viewer behavior: no path on open → cursor file; no path on close → close all
- [x] Implement confirmation dialog close (for canceling copy/mkdir)
- [x] Remove old dialog/settings tools

### Milestone 5: Other tools ✅

- [x] Implement unified `sort` tool with `pane`, `by`, `order` params
- [x] Remove old sort tools (8 tools)
- [x] Keep/rename: `switch_pane`, `toggle_hidden`, `set_view_mode`, `quit`
- [x] Update `copy` tool to return "waiting for user confirmation" message
- [x] Update `mkdir` tool similarly
- [x] Keep `refresh` tool

### Milestone 6: Response format ✅

- [x] Change all tool responses to plain text format (`OK: ...`, `ERROR: ...`)
- [x] Remove JSON wrappers from responses

### Milestone 7: Cleanup and docs ✅

- [x] Remove all deprecated tools from `tools.rs`
- [x] Remove all deprecated resources from `resources.rs`
- [x] Update `docs/features/mcp-server.md` to reflect new API
- [x] Run `./scripts/check.sh --rust` and fix any issues
- [ ] Test manually with MCP client (once app is running)
