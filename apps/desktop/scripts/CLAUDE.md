# Desktop build / dev scripts

Composition layer for `pnpm dev` and `pnpm build`. Owns the instance-isolation primitive (`CMDR_INSTANCE_ID`) at the
launch boundary, the llama-server fetch, and the type-drift check.

## Module map

- **`tauri-wrapper.ts`**: what `pnpm dev` / `pnpm build` call. Resolves `CMDR_INSTANCE_ID`, reserves ephemeral ports
  (Vite + tauri-MCP bridge), writes the generated `tauri.instance.json` to `$TMPDIR`, exports env, spawns Tauri.
  Dev-only: `CMDR_VIRTUAL_MTP` appends `--features virtual-mtp` (`docs/tooling/virtual-mtp.md`)
- **`instance-id.ts`**: pure helpers (slug sanitization, instance resolution, per-OS data-dir, bundle-id + productName
  - config-payload composition, port reservation, port-file write); `instance-id.test.ts` covers them in ~45 cases
- **`download-llama-server.go`**: build-time llama-server downloader, invoked from `src-tauri/build.rs`
- **`check-type-drift.ts`**: fast-lane check for hand-written types drifting from `bindings.ts`
- **`gen-shipped-locales.ts`** (+ `-lib.ts`): the Rust locale resolver's table of shipped catalogs and their CLDR
  scripts (`pnpm intl:shipped-locales`, guarded by `shipped-locales-fresh`)
- **`gen-native-strings.ts`** (+ `-lib.ts`): the catalog subset Rust draws itself (`menu.`, the window title, the
  already-running alert), read by `menu_t`. `pnpm intl:native-strings`, guarded by `native-strings-fresh`
- **`gen-boot-guard-lib.ts`**: the old-WebKit block screen's translated strings, spliced into the app shell by
  `svelte.config.js` at config-load time (no CLI, nothing committed). `src/lib/utils/DETAILS.md` § Old-WebKit boot guard
- **`i18n-*.ts`**: the catalog lib, the six locale checks, the pseudolocale/skeleton generators, key sync
- **`gen-analytics-defaults.ts`** (+ `-lib.ts`): the per-version settings-defaults manifest the dashboard resolves
  absent config keys against (`pnpm analytics:defaults`, guarded by `settings-defaults`); DETAILS § "The defaults
  manifest"
- **`marketing-shots.ts`** (+ `-thread.ts`): reshoots the brand masters (`pnpm marketing:shots`);
  `docs/guides/screenshots.md`. Needs ImageMagick, and a missing `magick` fails up front
- **`capture-runtime.ts`**: launch primitives shared by both capture orchestrators, plus `createTrackedArtifactGuard`
  (only a green run keeps its rewrite of tracked artifacts; DETAILS § "The capture guard")
- **`e2e-linux.sh`**: Linux Docker E2E launcher (`playwright-e2e,virtual-mtp`, single shard, legacy fixture)

Wrapper architecture, decisions, instance-isolation reference: `DETAILS.md`, `docs/tooling/instance-isolation.md`.

## Must-knows

- **Scripts are TypeScript run directly by Node** (`node scripts/foo.ts`), on Node 25's native type stripping, no build
  step. Two rules follow: a sibling import MUST carry the real `.ts` extension (`from './instance-id.ts'`; bare Node
  won't resolve a `.js` specifier to a `.ts` file), enabled by `allowImportingTsExtensions`; and stripping can't emit,
  so no `enum`/`namespace`/parameter properties/decorators. `console` is allowed here via an `eslint.config.js`
  override. The Go check runner invokes these by path, so renaming one means updating its caller there and in
  `package.json`.
- **Don't bypass the wrapper.** Raw `cargo tauri dev` / `cargo build` skips the env composition AND the
  `beforeBuildCommand` chain (llama-server download + frontend build), so the app launches with the prod identifier or
  no embedded frontend. See the `rust` rule in `.claude/rules/`.
- **`pnpm dev` refuses to run in the main clone** (a dev launch regenerates `bindings.ts` and runs the wrong instance;
  the workflow always devs from a worktree). Detection: `--git-dir` == `--git-common-dir` (`isMainWorkingTree`). `build`
  is exempt (CI runs in the main checkout); override with `--allow-main` / `-m`. Same guard in the check runner.
- **The generated `tauri.instance.json` lives in `$TMPDIR`, not the repo**, so a crashed wrapper can't pollute tracked
  space and `/tmp` self-prunes. Wrapper exit cleanup is best-effort and skips `SIGKILL`/OOM/terminal-close, so that
  location is the load-bearing fallback. Don't move it into the repo.
- **`download-llama-server.go` takes the binaries from the main clone in a worktree when `.version` matches**, as a
  COPY, ❌ never a symlink: the Linux-E2E container bind-mounts only the worktree, where such a symlink dangles. CI
  codesigns each one. DETAILS § "The llama-server fetch".
- **`--worktree` slug isn't validated against the directory name.** The wrapper sanitizes whatever slug you pass, so
  isolation can be pinned from a non-worktree shell.
- **`instance-id.ts` is stdlib-only** (`node:net`/`fs`/`os`/`path`/`child_process`), imported by `tauri-wrapper.ts` and
  the test. New helpers need a default-arg shape so existing wrapper code keeps working.
