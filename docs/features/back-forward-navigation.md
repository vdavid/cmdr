# Back/Forward navigation

Navigate between previously visited folders using browser-style back/forward functionality.

## Behavior

- **Per-pane history**: Each pane maintains its own independent back/forward stack
- **Session-only**: History resets when quitting the app (not persisted)
- **Cross-volume navigation**: History tracks navigation across different volumes, so you can go back/forward between
  volumes (for example, from an external drive back to Macintosh HD)
- **Network volume support**: When navigating to/from the Network virtual volume, history is preserved
- **Deleted folders**: When navigating to a folder that no longer exists, walks up the parent tree until finding an
  existing folder. If the entire volume is gone, skips to the next history entry.
- **History preservation**: Skipped entries remain in history (folder may become available again)

## Go menu

| Menu item       | Shortcut | Action                       |
| --------------- | -------- | ---------------------------- |
| `Back`          | `⌘[`     | Navigate to previous folder  |
| `Forward`       | `⌘]`     | Navigate to next folder      |
| `Parent folder` | `⌘↑`     | Navigate to parent directory |

**Note**: `Backspace` also navigates to parent (not shown in menu).

## Edge cases

| Scenario                    | Behavior                       |
| --------------------------- | ------------------------------ |
| Back at oldest entry        | No-op (stay in current folder) |
| Forward at newest entry     | No-op                          |
| Navigate after going back   | Forward history is cleared     |
| Target folder deleted       | Go to nearest existing parent  |
| Volume unmounted            | Skip to next history entry     |
| All history entries invalid | Stay in current folder         |
