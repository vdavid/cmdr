# Desktop build / dev scripts

Composition layer for `pnpm dev` and `pnpm build`. Owns the instance-isolation primitive (`CMDR_INSTANCE_ID`) at the
launch boundary, plus the llama-server fetch + the type-drift check.

## Files

- **`tauri-wrapper.js`**: The script `pnpm dev` and `pnpm build` actually call. Resolves `CMDR_INSTANCE_ID`, reserves
  ephemeral ports (Vite + tauri-MCP bridge), writes the generated `tauri.instance.json` to `$TMPDIR`, exports the right
  env, then spawns Tauri. Dev-only: when `CMDR_VIRTUAL_MTP` is set, appends `--features virtual-mtp` to the cargo build
  (see [`docs/tooling/virtual-mtp.md`](../../../docs/tooling/virtual-mtp.md))
- **`instance-id.js`**: Pure helpers backing the wrapper: slug sanitization, instance resolution, per-OS data-dir
  computation, bundle-identifier + productName + config-payload composition, ephemeral port reservation, port-file write
  protocol
- **`instance-id.test.js`**: Vitest suite for `instance-id.js`. ~45 cases covering every helper
- **`download-llama-server.go`**: Build-time downloader for the llama-server binary. Invoked from `src-tauri/build.rs`.
  In linked git worktrees, symlinks from the main clone when the `.version` matches. In CI release builds
  (`APPLE_SIGNING_IDENTITY` set) it codesigns each extracted binary; when `LLAMA_SIGN_KEYCHAIN` is set it targets that
  keychain explicitly via `codesign --keychain` (release.yml sets one up, in the search list, because the runner's
  launchd security session can't use the login keychain's key and `--keychain` alone doesn't work outside the search
  list)
- **`check-type-drift.ts`**: Fast-lane check that scans for hand-written types that drift from the auto-generated
  `bindings.ts`. Runs as part of `pnpm check --fast`
- **`e2e-linux.sh`**: Linux Docker E2E launcher. Builds the Tauri binary with `playwright-e2e,virtual-mtp` features,
  runs the suite. Single-shard; uses the legacy shared fixture path (no per-instance isolation)

## The wrapper architecture in one paragraph

`tauri-wrapper.js` is the single composition point for dev vs prod. Pure helpers in `instance-id.js` do the work so they
stay testable. For `pnpm dev`, the wrapper resolves an instance ID (from `--worktree <slug>`, the existing
`CMDR_INSTANCE_ID` env, or the default `"dev"`), reserves ephemeral ports, composes the bundle identifier +
productName + data dir + generated config payload, writes the config to a
`$TMPDIR/cmdr-tauri-instance-<rand>/tauri.instance.json` (NOT in the repo so a crashed wrapper can't pollute tracked
space), writes the tauri-MCP port file BEFORE Tauri launches (the plugin has no bound-port accessor, so external readers
learn the port from the wrapper), and exports `CMDR_DATA_DIR` + `CMDR_SECRET_STORE=file` for non-prod. Production leaves
`CMDR_INSTANCE_ID` unset and runs byte-identical to before instance isolation existed.

For the full design (per-resource derivation rules, race-window analysis, debug recipes, acceptance smoke), see
[`docs/tooling/instance-isolation.md`](../../../docs/tooling/instance-isolation.md).

## Key decisions

**Decision**: pure helpers in `instance-id.js`, side effects in `tauri-wrapper.js`. **Why**: the sanitizer, identifier
composer, port-file writer, and config-payload builder are all unit-testable in isolation. The wrapper itself is ~200
lines of obvious orchestration. Touching either side without breaking the other is the goal.

**Decision**: generated `tauri.instance.json` lives in `$TMPDIR`, not the repo. **Why**: a crashed wrapper leaves the
file behind. Tracked space is sacred; `/tmp` self-prunes on macOS. The `.gitignore` doesn't need an entry.

**Decision**: ephemeral Vite + tauri-MCP ports are picked by the wrapper via `net.createServer().listen(0)`, NOT by the
consumers. **Why**: the wrapper knows the data dir and can write the port file BEFORE the consumer spawns. The race
window (close → spawn → bind) is mitigated per-consumer: Vite uses `strictPort: true` so any collision is loud, the
Tauri-MCP plugin gets a post-bind connect-check on the Rust side that warns on mismatch.

## Gotchas

- **Don't bypass the wrapper.** Raw `cargo tauri dev` or raw `cargo build` skips the env composition AND the
  `beforeBuildCommand` chain (llama-server download + frontend build). The app launches with the prod identifier or no
  embedded frontend. See the `rust` rule in `.claude/rules/`.
- **Wrapper exit cleanup is best-effort.** `process.on('exit' / 'SIGINT' / 'SIGTERM')` doesn't run on `SIGKILL`, OOM, or
  VS Code closing the terminal. The `$TMPDIR` location is the load-bearing fallback so leaked configs auto-prune.
- **`--worktree` slug isn't validated against the actual worktree directory name.** The user pins whatever slug they
  want; the wrapper just sanitizes it. Useful if you're starting a dev session from a non-worktree shell but want
  isolation.

## Dependencies

- `node:net`, `node:fs`, `node:os`, `node:path`, `node:child_process` (stdlib only: no npm deps).
- `instance-id.js` exports are imported by both `tauri-wrapper.js` and `instance-id.test.js`. If you add a new helper,
  give it a default-arg shape so existing wrapper code keeps working.
