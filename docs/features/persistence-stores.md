# Persistence stores

Cmdr uses three persistent storage mechanisms, each for a distinct purpose. All are stored in the app's data directory
(on macOS: `~/Library/Application Support/com.cmdr.app/`).

## 1. App status store (`app-status.json`)

**Purpose**: Stores the current state of the application - where the user "is" in the app.

**File**: `src/lib/app-status-store.ts`

**Contents**:

- `leftPath`, `rightPath` - Current directory paths for each pane
- `focusedPane` - Which pane (`left` or `right`) has focus
- `leftViewMode`, `rightViewMode` - View mode (`full` or `brief`) for each pane
- `leftVolumeId`, `rightVolumeId` - Currently active volume for each pane
- `lastUsedPaths` - Map of `volumeId` -> last-used path; used to restore position when switching volumes

**Restored**: On app startup. Paths are validated and gracefully fall back to parent dirs or home if they no longer
exist.

## 2. Settings store (`settings.json`)

**Purpose**: Stores user preferences—how the user likes to work.

**File**: `src/lib/settings-store.ts`

**Contents**:

- `showHiddenFiles` - Whether to show hidden files (also synced with the View menu)
- `fullDiskAccessChoice` - Full disk access permission state (`allow`, `deny`, or `notAskedYet`)

**Restored**: On app startup. Settings can also be changed via the menu and are persisted immediately.

## 3. Window state (plugin-managed)

**Purpose**: Stores window size and position.

**File**: Managed by `@tauri-apps/plugin-window-state` (stores in a plugin-specific file)

**Contents**:

- Window position, size, and display

**Restored**: Automatically by the plugin on window creation. By default, saves on quit, but we also save on resize (see
`src/lib/window-state.ts`) to persist across hot reloads.

## Design philosophy

- **Status vs settings**: Status is ephemeral state (where you are), settings are preferences (how you like things). If
  the user resets "status", they should start fresh. If they reset "settings", they should get default preferences back.
- **Per-volume paths**: The `lastUsedPaths` map in app-status allows remembering where the user was on each volume.
  Switching volumes restores the last used directory. Favorites are shortcuts within volumes, so they don't store
  separate paths—they use the containing volume's path storage.
- **Graceful degradation**: All stores silently fail if persistence fails—the app works with defaults.
