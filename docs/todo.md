## Listing

- [ ] Add "change drive" feature
- [ ] Add different sorting options
- [ ] When sorting alphabetically, sort numbers ascending, not alphabetically
- [ ] Build a (set of) dmg release(s) and document the process
- [ ] Load iCloud sync statuses, too
- [ ] Load Google Drive sync statuses, too
- [ ] Load OneDrive sync statuses, too?
- [ ] Enable file drag&drop from the app to other apps.
- [ ] Test with slow drives like network drives

## Cleanup

- Fix calculating the Brief mode widths in Rust!
- Cancel requests in Rust when the dir is closed
- A round of refactoring is due
- Better test coverage to avoid regressions!

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
