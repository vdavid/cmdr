# Manual test checklist for releases

Use this checklist before each release to verify features that cannot be automatically tested.

> **Note:** Many features now have automated tests. This checklist focuses only on what requires manual verification due
> to macOS/Tauri limitations (native menus, drag-and-drop, real file system).

## Pre-release verification

### App startup

- [ ] App launches within 5 seconds
- [ ] Both panes display home directory contents
- [ ] No error dialogs on startup

### Context menu (native macOS—cannot be automated)

- [ ] Right-click shows native context menu
- [ ] "Open" works
- [ ] "Show in Finder" opens Finder
- [ ] "Copy path" copies full path to clipboard
- [ ] "Get info" opens system Info panel
- [ ] "Quick Look" previews file (space bar alternative)

### Drag-and-drop (Tauri plugin—cannot be automated)

- [ ] Drag file to Finder copies/moves it
- [ ] Drag file to other apps (like Mail) attaches it
- [ ] Drag icon shows file preview

### File watcher (requires real file system events)

- [ ] Create file in Finder → appears in Cmdr
- [ ] Delete file in Finder → disappears from Cmdr
- [ ] Rename file in Finder → updates in Cmdr

### Visual verification (requires human eye)

- [ ] Icons display correctly for common file types (.pdf, .txt, .app, folders)
- [ ] Symlink badge appears on symlinks
- [ ] Dropbox/iCloud sync icons appear in Dropbox/iCloud folders
- [ ] Columns in Brief mode fit filenames without truncation
- [ ] Loading indicator appears for large directories

### External volumes (requires physical hardware)

- [ ] External USB drive appears in volume picker
- [ ] Can navigate to external drive contents
- [ ] Network volumes appear (if mounted)
- [ ] Ejecting a volume gracefully switches pane to default

### Performance

- [ ] Directory with 1000 files loads in < 1 sec
- [ ] Directory with 50,000 files loads in < 5 sec
- [ ] UI remains responsive during loading
- [ ] Scrolling through 50k files is smooth (60 fps)
