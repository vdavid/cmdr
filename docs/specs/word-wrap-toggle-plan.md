# Word wrap toggle for viewer

## Context

The viewer clips long lines. Phase 1 (done) added horizontal scrolling. Phase 2 adds a word wrap toggle bound to "W" (no modifiers) with a menu item. Must work for all file sizes, which means the variable-height lines need to coexist with virtual scrolling.

## Approach

### Frontend: averaged-height virtual scroll

Keep the existing absolute-positioned virtual scroll. When word wrap is on:
- `.line` gets `height: auto` instead of `18px`
- `.line-text` gets `white-space: pre-wrap; overflow-wrap: break-word`
- `.lines-container` gets `width: auto; right: 0` (constrained to viewport for wrapping)
- `.file-content` gets `overflow-x: hidden`
- Use `effectiveLineHeight` (measured average when wrapping, fixed 18px when not) for all scroll calculations: total height, `visibleFrom`, `translateY`
- Lines within the visible chunk render in normal flow with natural heights; only the chunk's position vs the scroll spacer is approximate
- Measure the rendered chunk height per frame, compute average, update `effectiveLineHeight`
- This is the same level of approximation the byte-seek backend already uses for line numbers

Key state:
- `wordWrap: boolean` (default: false)
- `avgWrappedLineHeight: number` (starts at LINE_HEIGHT, updated from measurements)
- `effectiveLineHeight` derived: `wordWrap ? avgWrappedLineHeight : LINE_HEIGHT`

Toggle preserves scroll position by computing `targetLine = visibleFrom` and setting `scrollTop = targetLine * newEffectiveLineHeight`.

### Backend: per-window viewer menu

Use `Window::set_menu()` to give viewer windows their own menu. When a viewer is focused on macOS, its menu shows in the menu bar. The viewer menu starts from `Menu::default()` (standard macOS items: App, File, Edit, View, Window, Help) and adds "Word wrap" CheckMenuItem to the View submenu.

Two new Tauri commands:
- `viewer_setup_menu(label)` — builds and sets the viewer menu, called from `onMount`
- `viewer_set_word_wrap(label, checked)` — syncs menu check state when "W" is pressed in frontend

Menu event handling: in `on_menu_event`, when `viewer_word_wrap` is clicked, find the focused viewer window and emit `viewer-word-wrap-toggled` event to it.

### Files modified

| File | Change |
|---|---|
| `apps/desktop/src/routes/viewer/+page.svelte` | Word wrap state, CSS classes, averaged virtual scroll, "W" key, status bar, menu event listener |
| `apps/desktop/src/lib/tauri-commands/file-viewer.ts` | New command bindings: `viewerSetupMenu`, `viewerSetWordWrap` |
| `apps/desktop/src/lib/tauri-commands/index.ts` | Re-export new commands |
| `apps/desktop/src-tauri/src/menu.rs` | New `build_viewer_menu()` fn, new `VIEWER_WORD_WRAP_ID` const |
| `apps/desktop/src-tauri/src/commands/file_viewer.rs` | New `viewer_setup_menu` and `viewer_set_word_wrap` commands |
| `apps/desktop/src-tauri/src/lib.rs` | Register new commands, handle `viewer_word_wrap` in `on_menu_event` |

## Task list

### Milestone 1: Frontend word wrap toggle
- [x] Add `wordWrap`, `avgWrappedLineHeight`, `effectiveLineHeight` state/derived
- [x] Replace all `LINE_HEIGHT` scroll math with `effectiveLineHeight`
- [x] Handle "W" key in `handleKeyDown` (extracted to `handleToggleKey` for complexity)
- [x] Add `.word-wrap` CSS: `pre-wrap`, `height: auto`, `overflow-x: hidden`, `right: 0`
- [x] Add height measurement effect: measure chunk height, update `avgWrappedLineHeight`
- [x] Update status bar: show wrap badge + "W" shortcut hint
- [x] Reset `contentWidth` when toggling; preserve approximate scroll position

### Milestone 2: Menu integration (Rust)
- [x] Add `VIEWER_WORD_WRAP_ID` const and `build_viewer_menu()` to `menu.rs`
- [x] Add `viewer_setup_menu` and `viewer_set_word_wrap` commands to `commands/file_viewer.rs`
- [x] Register commands in `lib.rs` invoke handler
- [x] Handle `viewer_word_wrap` in `on_menu_event` (emit to focused viewer window)
- [x] Add frontend TS command bindings
- [x] Call `viewerSetupMenu` in viewer `onMount`
- [x] Listen for `viewer-word-wrap-toggled` event, toggle state + sync menu
- [x] On "W" key toggle, call `viewerSetWordWrap` to sync menu check state

### Milestone 3: Checks and testing
- [x] Run `./scripts/check.sh --svelte --rust` (all pass)
- [ ] Manual test: wrap on/off with small file, large file, byte-seek file
- [ ] Manual test: menu toggle syncs with "W" key
- [ ] Manual test: search highlighting works in both modes
- [ ] Manual test: resize window while word wrap is on

## Verification

1. Open viewer on a file with long lines
2. Verify horizontal scroll works (phase 1)
3. Press "W" — lines should wrap, horizontal scrollbar disappears, status bar shows wrap badge
4. Press "W" again — back to horizontal scroll
5. Click "Word wrap" in View menu — should toggle and stay in sync
6. Open a large file (> 1MB) — word wrap should work with approximate scroll
7. Search for text — highlights should appear correctly in both modes
8. Resize window while wrapped — lines re-wrap naturally
