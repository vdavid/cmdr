# Tauri capabilities: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the per-file design
rationale.

## Why one file per window

Each window type has a different trust level, and the capability system is the boundary controlling which PLUGIN and
`core:` APIs a webview reaches. The main window needs filesystem access, drag-and-drop, clipboard, and the updater. The
settings window only needs event dispatch and theme control. The viewer only needs window management. Splitting by
window means a compromised viewer webview can't reach the `fs` plugin, the store, or the updater.

**It does NOT cover our own commands.** Tauri ACL-checks an app-defined command only when the app ships an ACL manifest
(`src-tauri/permissions/`, which we don't have) or the call comes from a remote origin, so everything in
`generate_handler![]` is callable from every window. That's a deliberate position, not an oversight: the reasoning, the
assumption it rests on, and what would overturn it are in `docs/security.md` § "Why there's no caller-window
authorization guard". Don't write "a compromised viewer can't invoke X" about an app command.

## Debug window draws solely from `debug.json`

`default.json` is the most powerful capability in the app. The debug window must not inherit it: listing `"debug"` in
`default.json` would silently undo the per-window split for the most privileged capability. The debug window is dev-only
(frontend gates open on `import.meta.env.DEV`; the `mcp-bridge` plugin is `#[cfg(debug_assertions)]`), so the runtime
risk is low, but the structure is the foot-gun, any future gate slip would expose the full surface. The debug panels
only need core window/webview/event/app-theme ops, devtools, and `store:default` (they reach the backend through typed
app commands, which aren't ACL-gated, and through events), so `debug.json` carries `core:default` and is self-contained.

## Viewer settings persistence path

Because the viewer has no store access (see `CLAUDE.md`), viewer settings persist through the typed restricted-window
command pair in `commands/settings.rs`:

- `get_restricted_window_settings`: read allowlist (word wrap, binary-warning suppression, text size, app color).
- `persist_restricted_window_setting`: write allowlist, a typed enum covering only `viewer.wordWrap` and
  `fileViewer.suppressBinaryWarning`, forwarded to the main window's `restricted-settings-bridge.ts`, which re-checks
  the allowlist before persisting through the normal store pipeline.

The enum is the boundary: a compromised viewer can flip those two booleans and nothing else. Viewer tail mode stays
deliberately unpersisted (defaults off per session, see `routes/viewer/CLAUDE.md` § Tail mode).

## The E2E capability

`playwright:default` exists only when `tauri-plugin-playwright` is linked, so a capability naming it fails validation in
every build without the `playwright-e2e` feature: `Permission playwright:default not found`. It therefore can't live in
`capabilities/`, which every build globs.

It sits in the sibling `capabilities-e2e/playwright.json` instead, and `build.rs` widens the glob for feature builds
only:

- Without the feature: plain `tauri_build::build()`, so a production build takes tauri-build's default
  `./capabilities/**/*` path completely unchanged.
- With the feature: `try_build` with `capabilities_path_pattern("./capabilities*/**/*")`, covering both directories. A
  sibling rather than a subdirectory because the `glob` crate has no brace expansion, and because it leaves the pattern
  every other build uses alone.

**Nothing writes into the source tree at build time**, which is the point. ❌ Don't go back to generating the file under
the feature and deleting it without: `capabilities/` is shared by every cargo process in the worktree, and the rustdoc
lane runs `cargo doc --all-features` in its own target directory precisely so that it runs BESIDE clippy and the test
lanes. Two invocations with different features then race over one file, and `pnpm check rust` flakes.

**⚠️ `build.removeUnusedCommands` interacts with this.** Cmdr doesn't set it. It strips plugin commands that no
capability grants, and each build script computes its own list: `tauri-build` passes the custom-globbed capabilities for
the app's own commands, but `tauri-plugin`'s build script passes `None` and falls back to the default
`capabilities/**/*` (`tauri-plugin/src/build/mod.rs`). A capability outside `capabilities/` is therefore invisible to
plugin build scripts. Turning the option on would strip the playwright plugin's commands from the E2E build; production
is unaffected, since every production capability is in `capabilities/`. The fix would be merging
`removeUnusedCommands: false` into the E2E build through its own `--config`.
