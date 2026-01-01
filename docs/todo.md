## Listing

- [ ] We need to ask for permissions for `Downloads`, etc. Handle permission denial gracefully!
- [ ] Add different sorting options
- [ ] When sorting alphabetically, sort numbers ascending, not alphabetically
- [ ] Make it handle network drives
- [ ] Test with slow drives like network drives
- [ ] Load iCloud sync statuses, too
- [ ] Load Google Drive sync statuses, too
- [ ] Load OneDrive sync statuses, too?
- [ ] Read the "dataless" flag for Dropbox/Drive files to avoid triggering a massive download when iterating through the
      files later, to generate thumbnails or whatnot. Files are only placeholders in this case: they have a file size in
      `stat`, but zero bytes on disk.

## Cleanup

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
- [ ] Add an action palette like VS Code's etc.

## File viewer

- Add "View" to File menu and context menu

## Add AI features

Ideas

- [ ] Smart selection: Instead of RegEx or glob, "Select all error logs from last week that mention 'timeout'.", or
      "Select all Typescript files that haven't been modified in 6 months and have no imports." -> "Move to /archive".
- [ ] Select 50 screenshots (ScreenShot 2026-01...). → "Rename these based on what is visible in the pic." →
      "Login_Page_Error.png", "Dashboard_Dark_Mode.png".
- [ ] "Organize this" Button: Apply in "Downloads" folder → AI analyzes types and contents, proposes a structure.
- [ ] "Explain this" in context menu: Right-click a minified JS file, a binary, or a cryptic config -> "Explain what
      this does."
- [ ] Add a small local LLM for privacy-conscious users.
