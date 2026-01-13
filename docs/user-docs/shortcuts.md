# Keyboard shortcuts

This document lists all keyboard shortcuts available in Cmdr.

## Navigation

### Basic navigation

| Shortcut           | Action                          | Mode       |
| ------------------ | ------------------------------- | ---------- |
| `↑`                | Move selection up one item      | Both       |
| `↓`                | Move selection down one item    | Both       |
| `←`                | Move selection left one column  | Brief only |
| `→`                | Move selection right one column | Brief only |
| `Enter`            | Open selected file/folder       | Both       |
| `Backspace` / `⌘↑` | Navigate to parent directory    | Both       |

### Jump shortcuts

| Shortcut | Action                    | Mode |
| -------- | ------------------------- | ---- |
| `⌥↑`     | Jump to first item (Home) | Both |
| `⌥↓`     | Jump to last item (End)   | Both |
| `Fn←`    | Jump to first item (Home) | Both |
| `Fn→`    | Jump to last item (End)   | Both |
| `Fn↑`    | Page up                   | Both |
| `Fn↓`    | Page down                 | Both |

**Note**: On macOS, `Fn+Arrow` keys generate `Home`, `End`, `PageUp`, and `PageDown` key events.

### Pane navigation

| Shortcut | Action                              |
| -------- | ----------------------------------- |
| `Tab`    | Switch between left and right panes |

## Notes

- **Brief mode**: The file list is displayed in multiple columns. Arrow keys navigate within and between columns.
    - Page Up/Down: Moves horizontally by (number of visible columns - 1) and jumps to the bottommost item in the target
      column. If the target would be at or past the leftmost/rightmost edge, it jumps to the first/last item instead.
      This allows quick navigation across large file sets while maintaining context.
- **Full mode**: The file list is displayed in a single column with detailed metadata (size, date). Only up/down arrow
  keys navigate items; left/right arrows are not used.
    - Page Up/Down: Moves vertically by (number of visible items - 1), adapting to the current window size.
- All jump shortcuts (Home/End) work consistently in both Brief and Full modes.

## Shortcuts by category

### Quick jumps

- Go to start: `⌥↑` or `Fn←`
- Go to end: `⌥↓` or `Fn→`
- Page up: `Fn↑` (Brief: move left by visible columns - 1, or to first item if near edge; Full: move up by visible
  items - 1)
- Page down: `Fn↓` (Brief: move right by visible columns - 1, or to last item if near edge; Full: move down by visible
  items - 1)

### File operations

- Open file/folder: `Enter`
- Go up one directory: `Backspace` or `⌘↑`

### Interface

- Switch panes: `Tab`
