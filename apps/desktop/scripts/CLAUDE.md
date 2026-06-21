# Desktop build / dev scripts

Composition layer for `pnpm dev` and `pnpm build`. Owns the instance-isolation primitive (`CMDR_INSTANCE_ID`) at the
launch boundary, plus the llama-server fetch and the type-drift check.

## Module map

- **`tauri-wrapper.js`**: what `pnpm dev` / `pnpm build` actually call. Resolves `CMDR_INSTANCE_ID`, reserves ephemeral
  ports (Vite + tauri-MCP bridge), writes the generated `tauri.instance.json` to `$TMPDIR`, exports env, spawns Tauri.
  Dev-only: with `CMDR_VIRTUAL_MTP` set, appends `--features virtual-mtp` (see
  [`docs/tooling/virtual-mtp.md`](../../../docs/tooling/virtual-mtp.md))
- **`instance-id.js`**: pure helpers (slug sanitization, instance resolution, per-OS data-dir, bundle-id + productName +
  config-payload composition, port reservation, port-file write)
- **`instance-id.test.js`**: Vitest suite (~45 cases) for `instance-id.js`
- **`download-llama-server.go`**: build-time llama-server downloader, invoked from `src-tauri/build.rs`
- **`check-type-drift.ts`**: fast-lane check for hand-written types drifting from `bindings.ts`; runs in
  `pnpm check --fast`
- **`e2e-linux.sh`**: Linux Docker E2E launcher (`playwright-e2e,virtual-mtp` features, single shard, legacy shared
  fixture path)

Wrapper architecture, decisions, and the full instance-isolation reference: [DETAILS.md](DETAILS.md) and
[`docs/tooling/instance-isolation.md`](../../../docs/tooling/instance-isolation.md).

## Must-knows

- **Don't bypass the wrapper.** Raw `cargo tauri dev` or raw `cargo build` skips the env composition AND the
  `beforeBuildCommand` chain (llama-server download + frontend build), so the app launches with the prod identifier or
  no embedded frontend. See the `rust` rule in `.claude/rules/`.
- **`pnpm dev` refuses to run in the main clone** (a dev launch regenerates `bindings.ts` and runs the wrong instance;
  the workflow always devs from a worktree). Detection: `--git-dir` == `--git-common-dir` (`isMainWorkingTree`). `build`
  is exempt (CI release builds run in the main checkout); override with `--allow-main` / `-m`. The check runner carries
  the same guard.
- **The generated `tauri.instance.json` lives in `$TMPDIR`, not the repo**, so a crashed wrapper can't pollute tracked
  space; `/tmp` self-prunes. Wrapper exit cleanup (`process.on('exit'/'SIGINT'/'SIGTERM')`) is best-effort and doesn't
  run on `SIGKILL`/OOM/terminal-close, so the `$TMPDIR` location is the load-bearing fallback. Don't move it into the
  repo.
- **`download-llama-server.go` symlinks from the main clone in linked worktrees when `.version` matches** (else
  downloads). In CI release builds (`APPLE_SIGNING_IDENTITY` set) it codesigns each extracted binary; when
  `LLAMA_SIGN_KEYCHAIN` is set it passes `codesign --keychain` explicitly (release.yml puts that keychain in the search
  list because the runner's launchd session can't use the login keychain's key and `--keychain` alone doesn't work
  outside the search list).
- **`--worktree` slug isn't validated against the actual worktree directory name.** The wrapper just sanitizes whatever
  slug you pass, so you can pin isolation from a non-worktree shell.
- **`instance-id.js` is stdlib-only** (`node:net`/`fs`/`os`/`path`/`child_process`, no npm deps) and is imported by both
  `tauri-wrapper.js` and the test. New helpers need a default-arg shape so existing wrapper code keeps working.
