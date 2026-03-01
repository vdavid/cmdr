# Linux UI foundations: menus, window controls, and shortcuts

## Context

The app builds and runs on Linux, but three core UI systems are macOS-only:

1. **Window buttons don't work** — `titleBarStyle: "Overlay"` + `trafficLightPosition` in `tauri.conf.json` is
   macOS-specific. On Linux/GTK, overlay mode hides the native title bar without providing any window controls.
2. **Menus are broken** — `Menu::default()` in `menu.rs` creates a macOS-style default menu. The code then patches
   submenus by name ("cmdr", "File", "Edit", "View", "Window") — some of which may not exist on Linux. Also, several
   menu items are macOS-specific ("Show in Finder", "Quick Look", "Get info").
3. **Shortcuts don't work** — The command registry (`command-registry.ts`) stores all shortcuts as macOS symbols
   (`⌘Q`, `⌘,`). The dispatch map (`shortcut-dispatch.ts`) uses these as lookup keys. On Linux, `formatKeyCombo()`
   produces `Ctrl+Q`, `Ctrl+,` — no match against the `⌘` keys. Shortcuts are completely broken.

## Approach

### 1. Window title bar and decorations

Move macOS-specific window config from `tauri.conf.json` to Rust code in `setup()`. On Linux, the window gets standard
GTK decorations with native close/min/max buttons. The custom `.title-bar` div is hidden on Linux — the native title bar
already provides title, drag, and buttons.

### 2. Menu system

Build the menu from scratch on Linux (no `Menu::default()` patching). Platform-aware accelerators and labels. Hide
macOS-only items (Quick Look, Get Info) on Linux.

### 3. Shortcut dispatch

Add `toPlatformShortcut()` to convert macOS symbols to platform format on non-macOS. Apply in
`getEffectiveShortcuts()`. Special handling for `⌃⌘` collision (both map to Ctrl on Linux).

### 4. File action commands

Implement `show_in_finder` and `open_in_editor` for Linux using `xdg-open`. Keep `quick_look` and `get_info` as stubs.

### 5. Platform-specific labels

Update command registry to use platform-aware names ("Show in Finder" → "Show in file manager" on Linux).

## Milestones

### M0a: Window decorations
- [ ] Move macOS-only window config from `tauri.conf.json` to Rust `setup()` code
- [ ] On Linux, use standard GTK decorations (native title bar with buttons)
- [ ] Conditionally hide the custom `.title-bar` div on Linux in `+page.svelte`

### M0b: Shortcut dispatch
- [ ] Add `toPlatformShortcut()` to `key-capture.ts` — converts `⌘Q` → `Ctrl+Q` on non-macOS
- [ ] Handle the `⌃` + `⌘` collision (both → Ctrl on Linux)
- [ ] Apply conversion in `getEffectiveShortcuts()` for registry defaults
- [ ] Update `command-registry.ts` labels to be platform-aware ("Show in file manager" on Linux)

### M0c: Menu system
- [ ] Build Linux menu from scratch (no `Menu::default()` patching)
- [ ] Use platform-appropriate accelerator strings (handle `Ctrl+Cmd` collision)
- [ ] Hide macOS-only menu items on Linux (Quick Look, Get Info)
- [ ] Rename "Show in Finder" → "Show in file manager" in menu
- [ ] Add Settings to Edit menu on Linux

### M0d: File action commands
- [ ] Implement `show_in_finder` for Linux using `xdg-open` on parent dir
- [ ] Implement `open_in_editor` for Linux using `xdg-open`
- [ ] Wire up Linux implementations in `execute_menu_action`

### Verification
- [ ] Run `./scripts/check.sh --svelte` — frontend checks pass
- [ ] Run Rust checks
