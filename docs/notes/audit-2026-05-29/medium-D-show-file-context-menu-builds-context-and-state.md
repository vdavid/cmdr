# `show_file_context_menu` couples LaunchServices, sync-status, FP-domain, and state-stash into one command

**Severity:** medium
**Lens:** D — IPC boundary
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/commands/ui.rs:30-84` (`show_file_context_menu` command)
- `apps/desktop/src-tauri/src/commands/ui.rs:86-111` (`build_file_context_info` helper)

## What

The Tauri command does six things, each non-trivial:

1. Parses and normalises the `paths` vec (`context_paths = if paths.is_empty() { vec![path.clone()] } else { paths }`).
2. Calls `build_file_context_info`, which itself:
   - Constructs a `PathBuf`
   - Queries File Provider iCloud-domain membership (`is_in_icloud_drive`)
   - Fetches per-path sync status (`get_sync_statuses`)
   - Invokes LaunchServices for "Open with" candidates (50-200 ms on a cold cache, by the inline comment)
3. Locks `MenuState`, stashes `path`, `filename`, and `paths` into it.
4. Conditionally (macOS) clears the `open_with_apps` HashMap.
5. Calls `build_context_menu`, then locks MenuState a second time to stash the returned `open_with_apps` map.
6. Pops the menu.

`commands/CLAUDE.md` says: "No business logic here. If you find yourself adding branching or data transformation, move
it to the relevant subsystem module." This command lives well past that line — it's the orchestration layer for the
file-context-menu feature, not an IPC shell.

The same pattern appears in `show_breadcrumb_context_menu` (lines 122-150) at lower intensity (two state writes, one
menu build), but the file-context one is the largest offender.

## Why it matters

1. **Concurrency exposure.** Two separate `MenuState` lock acquisitions across an `await`-less but I/O-heavy
   `build_context_menu` call create a window where two concurrent right-clicks (palette + mouse, or chord + mouse)
   would interleave reads/writes on `open_with_apps`. Today the chance is low because right-clicks aren't naturally
   concurrent, but the code shape doesn't prevent it.
2. **Untestable.** None of the orchestration — paths normalisation, FP-domain detection, sync-status batch query,
   LaunchServices query, MenuState transition — can be exercised without a running Tauri app. Splitting the data
   construction into a pure `prepare_file_context_menu_inputs(...)` in `crate::menu` (or `crate::file_system::
   context_menu`) makes it directly unit-testable.
3. **Hides a 50-200 ms LaunchServices call inside a command shell.** The inline comment acknowledges the slow path.
   Treating it as business logic in the subsystem module would also let it live behind the existing
   `blocking_with_timeout` / `spawn_blocking` discipline that the rest of the file already uses for slow filesystem
   calls.

## Evidence

`ui.rs:30-84`:

```rust
pub fn show_file_context_menu<R: Runtime>(
    window: Window<R>, path: String, filename: String,
    is_directory: bool, paths: Vec<String>, restrict_destination_actions: bool,
) -> Result<(), String> {
    let app = window.app_handle();
    let context_paths = if paths.is_empty() { vec![path.clone()] } else { paths };

    #[cfg(target_os = "macos")]
    let info = build_file_context_info(&path, &context_paths);
    …
    // First MenuState write
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = path.clone();
        context.filename = filename.clone();
        context.paths = context_paths;
        … context.open_with_apps.clear();
    }

    let result = build_context_menu(app, &filename, is_directory, &info, restrict_destination_actions)
        .map_err(|e| e.to_string())?;

    // Second MenuState write
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.open_with_apps = result.open_with_apps;
    }

    result.menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}
```

Doc rule violated (`commands/CLAUDE.md` § "No business logic here"):

> If you find yourself adding branching or data transformation, move it to the relevant subsystem module.

## Suggested fix

1. Add `crate::menu::show_file_context_menu(app, window, args)` (or `crate::file_system::context_menu::show_file_…`)
   that owns the entire flow: input normalisation, info collection, MenuState transition, menu build, popup.
2. Inside that subsystem function, factor the side-effect-free parts into a pure helper
   (`prepare_file_context_menu_inputs(primary_path, all_paths) -> FileContextInputs`) so the orchestration is testable
   without LaunchServices/MenuState dependencies — exactly the test-independence the doc rule cites as the reason for
   the rule.
3. Reduce the Tauri command to:

   ```rust
   pub fn show_file_context_menu<R>(window: Window<R>, args: ShowFileMenuArgs) -> Result<(), String> {
       crate::menu::show_file_context_menu(window.app_handle(), window, args).map_err(|e| e.to_string())
   }
   ```

4. Bundle the six positional args into a single `ShowFileMenuArgs` struct (specta-typed) so the call site reads
   `commands.showFileContextMenu({ path, filename, isDirectory, paths, restrictDestinationActions })`.
   This also fixes the related raw-`invoke` call site noted in
   `medium-D-raw-invoke-call-sites-outside-documented-exclusions.md`.

Apply the same refactor pattern to `show_breadcrumb_context_menu`, `show_tab_context_menu`, and
`show_network_host_context_menu` — they all carry the same MenuState-write-then-popup shape, just smaller.

## Notes

Smaller fat-command offenders in the same lens, worth fixing in the same pass: `set_menu_context` (ui.rs:639-693,
iterates MenuState items and branches on item IDs), `update_view_mode_menu` (ui.rs:236-272, parses string enums and
gates a full menu rebuild), `update_known_share` (network.rs:158-177, builds a `KnownNetworkShare` struct inline from
five positional args).
