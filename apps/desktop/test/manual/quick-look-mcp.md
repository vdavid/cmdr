# Quick Look MCP smoke procedure

The native `QLPreviewPanel` lives outside the webview, so Playwright can't see it. This procedure drives the state-flip
parts of Quick Look (the most likely things to silently break in refactors) via the `cmdr` + `tauri` MCP servers. Run it
after touching anything in `src-tauri/src/quick_look/`, `commands/ui.rs::quick_look_*`, or
`src/lib/file-explorer/quick-look/`.

The native panel rendering itself isn't covered — verify that visually per
[`docs/specs/quick-look-plan.md`](../../../../docs/specs/quick-look-plan.md) § "Test plan (manual)".

## Prerequisites

- `pnpm dev` running (Cmdr dev build on macOS).
- Both MCP servers reachable (see [`docs/tooling/mcp.md`](../../../../docs/tooling/mcp.md)).
- A test directory with at least one local file the FE can put the cursor on (a `.png` or `.txt` works).

## Procedure

1. **Navigate to a folder with at least one file.** Use the cmdr MCP:

   ```
   mcp__cmdr__nav_to_path { pane: "left", path: "<absolute-path-to-folder>" }
   ```

   Confirm via `cmdr://state` that the cursor lands on a real entry.

2. **Open Quick Look via the command palette dispatch path.**

   ```
   mcp__tauri__webview_execute_js {
     code: "import('/src/routes/(main)/command-dispatch.ts').then(m => m.handleCommandExecute('file.quickLook'))"
   }
   ```

   Or — equivalent and simpler if the command palette is wired:

   ```
   mcp__cmdr__search { pattern: "Quick look" }  # via the command palette flow
   ```

   (Whatever the project's recommended palette-dispatch entry is at the time. The dispatch path is the unit under test;
   the entry point doesn't matter.)

3. **Assert `isOpen === true`.**

   ```
   mcp__tauri__webview_execute_js {
     code: "return (await import('/src/lib/file-explorer/quick-look/quick-look-state.svelte.ts')).quickLookState.isOpen"
   }
   ```

   Expected: `true`. If `false`, the IPC call or the dispatch path is broken.

4. **Close via the IPC wrapper.**

   ```
   mcp__tauri__webview_execute_js {
     code: "return (await import('/src/lib/tauri-commands/file-actions.ts')).quickLookClose()"
   }
   ```

5. **Wait for the close event and re-assert `isOpen === false`.**

   The Rust side emits `quick-look-closed` once AppKit animates the panel out. Poll for ~500 ms:

   ```
   mcp__tauri__webview_execute_js {
     code: `
       const start = performance.now();
       while (performance.now() - start < 500) {
         const s = (await import('/src/lib/file-explorer/quick-look/quick-look-state.svelte.ts')).quickLookState;
         if (!s.isOpen) return true;
         await new Promise(r => setTimeout(r, 25));
       }
       return false;
     `
   }
   ```

   Expected: `true` (meaning `isOpen` flipped back to `false`). If `false`, the close event listener is broken or the
   Rust observer didn't emit.

6. **Reopen and re-target.**
   - Repeat step 2 to reopen.
   - Move the cursor to a different file:
     ```
     mcp__cmdr__move_cursor { pane: "left", direction: "down" }
     ```
   - Wait ~150 ms (the cursor-follow `$effect` debounces at 100 ms) and confirm the controller's `current_url` updated.
     There's no IPC getter for `current_url`, but the next `set_path` call is the contract: confirm via logs that
     `panel opened for` (initial open) is followed by no errors and a fresh `set_path` log line. Use:
     ```
     mcp__cmdr__read_resource { uri: "cmdr://logs?filter=quick_look&limit=20" }
     ```

7. **Close cleanly.** Repeat step 4 so the next run starts from a clean state.

## What this does NOT cover

- The native panel becoming key, the menu bar saying "Cmdr", the panel rendering thumbnails — visual checks live in
  [`docs/specs/quick-look-plan.md`](../../../../docs/specs/quick-look-plan.md) § "Test plan (manual)".
- The AppKit-side close paths (✕ button, Esc). The Rust observer fires `quick-look-closed` for all three close routes;
  this procedure covers the IPC-initiated close, and the other two routes share the same observer codepath.
- Key forwarding (`quick-look-key` events). The Vitest spec
  (`apps/desktop/src/lib/file-explorer/quick-look/quick-look-state.test.ts`) covers the receive-side; the emit side is
  AppKit-driven and only verifiable manually (hold ArrowDown over a multi-file folder with the panel open — preview
  should follow).

## When something fails

- `isOpen` stuck at `true` after step 5: the Rust observer didn't emit `quick-look-closed`. Check
  `register_close_observer` in `controller.rs` and the `quickLookPanelWillClose:` selector wiring.
- `isOpen` stays `false` after step 2: the IPC call returned without effect. Check the `volume_supports_local_fs` gate
  (logs at debug `target: "quick_look"`) — the test folder might be on a volume the gate rejects.
- The `mcp__tauri__webview_execute_js` calls themselves fail: dev server isn't running, or the module paths in the
  `import(...)` calls have drifted. Check the actual paths under `apps/desktop/src/`.
