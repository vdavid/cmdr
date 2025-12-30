## UI

- [x] Add dark mode
- [x] Automate dark mode
- [x] Remember last window size

## Listing

- [x] Add file navigation (up/down) functionality
- [x] Add `..` (up one folder) item
- [x] Add navigation feature (go in dir, go up one dir)
- [x] Add chunked loading for big folders or slow drives for fast first byte experience
- [x] Improve performance
- [x] Display file info below each panel for the file under the cursor
- [x] Add context menu for files and folders
- [x] Add file watching to auto-update changes. It should be as close to immediate as possible
- [x] Implement proper Full view (with fixed columns)
- [x] Add Brief view with view switching option
- [x] Make sure it lists Dropbox files correctly, incl. files that are loaded on the fly
- [ ] Build a (set of) dmg release(s) and document the process
- [ ] Add different sorting options
- [ ] When sorting alphabetically, sort numbers ascending, not alphabetically
- [ ] Add "change drive" feature
- [ ] Tweak chunked loading to load in chunks on the backend based on drive speed. So far, we've only tested with fast
      drives, and chunk sizes are constant.
- [ ] Load iCloud sync statuses, too
- [ ] Load Google Drive sync statuses, too
- [ ] Load OneDrive sync statuses, too?
- [ ] Enable file drag&drop from the app to other apps.
- [x] `⌥↑` and `⌥↓` should be home/end, `Fn←` and `Fn→` same, then `Fn↑` and `Fn--↓` should be page up/down.

## Cleanup

- In Full mode, size display coloring is ugly, fix it
- When the app starts, it's temporarily all white bg for like 1 second. Go around this by delaying opening the window,
  or show a nice loading screen via normal HTML (no svelte) if that solves it. What can we do?
- Big dir reading is wrong: takes 7 seconds for the 50k dir, and it looks weird: it shows 45k files Loading but then it
  loads immediately
- Split-second "Loading..." state in panes at each dir change, ugly. → In "Loading" state, display empty div for 200 ms,
  and just THEN show "Loading..." if needed.
- Fix calculating the Brief mode widths in Rust!
- Cancel requests in Rust when the dir is closed
- A round of refactoring is due

## Settings

- [ ] Add settings window
- [ ] Add settings to menu
- [ ] Add quick actions menu
- [ ] Add toggle for showing/hiding hidden files (files starting with '.')
- [ ] Make sorting configurable (by name, size, date, etc.)

## Actions

- [ ] Add file selection feature
- [ ] Add copy, move, delete functionality
- Add these to the context menu:
    - 🟢 Easy Rename 2 Text input + fs.rename() calls already exist
    - 🟢 Easy New Folder 2 Already have F7 likely, just wire to menu
    - 🟡 Medium Delete permanently 3 Need confirmation dialog, already have delete logic?
    - 🟡 Medium Edit (F4) 4 Open in default editor via shell.open()
    - 🟡 Medium Duplicate 4 Copy + rename with "(copy)" suffix
    - 🟡 Medium Make Symlink 5 std::os::unix::fs::symlink - straightforward
    - 🟠 Hard Compress selected file(s) 6 Need to call zip or use a Rust crate
    - 🟠 Hard Color tags (macOS) 7 Requires extended attributes - xattr crate
    - 🟠 Hard Tags... dialog 7 UI for managing tags + xattr integration

## File viewer

- Add "View" to File menu and context menu
