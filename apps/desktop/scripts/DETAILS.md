# Desktop scripts details

Depth and rationale. `CLAUDE.md` holds the must-knows; the wrapper architecture and decisions live here. The canonical
instance-isolation reference (per-resource derivation, race-window analysis, debug recipes, acceptance smoke) is
`docs/tooling/instance-isolation.md`.

## The wrapper architecture

`tauri-wrapper.ts` is the single composition point for dev vs prod. Pure helpers in `instance-id.ts` do the work so they
stay testable. For `pnpm dev`, the wrapper resolves an instance ID (from `--worktree <slug>`, the existing
`CMDR_INSTANCE_ID` env, or the default `"dev"`), reserves ephemeral ports, composes the bundle identifier + productName

- data dir + generated config payload, writes the config to a `$TMPDIR/cmdr-tauri-instance-<rand>/tauri.instance.json`
  (NOT in the repo, so a crashed wrapper can't pollute tracked space), writes the tauri-MCP port file BEFORE Tauri
  launches (the plugin has no bound-port accessor, so external readers learn the port from the wrapper), and exports
  `CMDR_DATA_DIR` + `CMDR_SECRET_STORE=file` for non-prod. Production leaves `CMDR_INSTANCE_ID` unset and runs
  byte-identical to before instance isolation existed.

### Dev forces the MCP server on (`CMDR_MCP_ENABLED=1`)

For `pnpm dev` launches the wrapper exports `CMDR_MCP_ENABLED=1` unless the env var is already set. Without it, MCP is
silently dead across dev sessions: the FE persists `developer.mcpEnabled: false` (the registry default) as an explicit
saved value on first run, and that saved `false` then beats the debug-build-on default (`cfg!(debug_assertions)` in
`mcp/config.rs`) at every later launch. `mcp/config.rs::from_settings_and_env` reads `CMDR_MCP_ENABLED` ahead of the
setting, so the export wins; `CMDR_MCP_ENABLED=0` still exercises the off path. The deeper fix — not persisting registry
defaults as explicit choices — is a known settings-store follow-up (see `src-tauri/src/settings/DETAILS.md`).

## The llama-server fetch

`download-llama-server.go` runs from `src-tauri/build.rs` and puts the llama-server binaries where the build expects
them. In a linked worktree whose `.version` matches the main clone's, it takes them from there instead of the network:
an APFS clonefile (`cp -c`), falling back to a plain copy, and downloading only when neither is possible.

- **A copy, never a symlink into the main clone.** The Linux-E2E Docker container bind-mounts only the worktree, so a
  symlink pointing outside it dangles there and breaks the in-container build.
- **CI release builds codesign each extracted binary**, detected by `APPLE_SIGNING_IDENTITY` being set. When
  `LLAMA_SIGN_KEYCHAIN` is set the script passes `codesign --keychain` explicitly, and `release.yml` ALSO puts that
  keychain in the search list: the runner's launchd session can't reach the login keychain's key, and `--keychain` on
  its own doesn't work for a keychain outside the search list.

## The capture guard

`capture-runtime.ts`'s `createTrackedArtifactGuard` is why a half-finished i18n capture can't leave the repo claiming a
full one happened. `i18n-capture.ts` wraps the two tracked artifacts the run rewrites, `capture-report.json` and
`capture-skipped.json`, which is what the message-screenshot couplings read: a partial report would name surfaces the
run never photographed, and the repo would treat that as true. The screenshots themselves aren't guarded (regenerable),
and an overflow pass writes to its own gitignored subdir, so it guards nothing at all.

The shape is snapshot → run → `earn()` only on a complete green finish, with `restoreUnlessEarned()` on `process.exit`
putting the previous contents back (removing a file that wasn't there before). An untouched file is left alone so a
green-but-identical run doesn't churn mtimes, and the snapshot is dropped either way so a second call can't undo a later
write.

`earn()` is also where `i18n-capture.ts` runs `oxfmt` over the two artifacts. The spec writes them with plain
`JSON.stringify(…, 2)`, which oxfmt reflows, so without that step every capture run hands the next `pnpm check` a format
diff in a tracked file nobody hand-edited. Formatting AFTER `earn()` keeps the guard honest: a rolled-back run restores
the old bytes and never reformats anything.

**A DELETED surface leaves the report by hand.** The main pass builds its map from `{}`, but the license and FDA passes
load the committed report and merge into it, so a surface that no longer exists anywhere (a dialog case that was
removed) survives until the next full main capture — and until then the report describes a screen the app can't show.
`couple-screenshots.ts --check` stays green over it ("Skipping … not present", exit 0), so nothing surfaces it. Deleting
the surface's object out of `capture-report.json` is safe and is what the next full run would do; check first whether
any catalog key's `@key.screenshot` names that PNG (`grep` the `en/` catalogs), and re-run `pnpm i18n:couple` if one
does, so the key falls back to the next surface it appeared on rather than pointing at an image nobody will capture
again.

## The defaults manifest

`gen-analytics-defaults.ts` writes `apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json`: what a fresh
install runs with, per released app version, for exactly the keys the heartbeat's config shape can carry. The dashboard
joins it on `app_version` to tell "the user is on the default" apart from "the setting didn't exist in that build".

Two things about it are load-bearing:

- **The registry is PARSED, not imported.** `definitions/appearance.ts` pulls in `$lib/intl`, whose `messages.svelte.ts`
  is rune-compiled, so a bare `node` import can't evaluate the registry. Reading the definition objects off the
  TypeScript AST needs no Svelte toolchain, and it works against a `git show` of a release tag, which is what makes
  `--backfill` able to reconstruct the history from the tags.
- **`next` is not a version.** Between releases `package.json` still holds the LAST shipped version, so keying the
  working tree's snapshot by it would rewrite an entry that describes a release already in the field. `next` holds the
  unreleased state, is never resolved against, and `scripts/release.sh` promotes it under the real version number.

The two locale generators emit `#[rustfmt::skip]`, so they own their layout and need no Rust toolchain.

## Key decisions

- **Pure helpers in `instance-id.ts`, side effects in `tauri-wrapper.ts`.** The sanitizer, identifier composer,
  port-file writer, and config-payload builder are all unit-testable in isolation. The wrapper is ~200 lines of obvious
  orchestration. Touching either side without breaking the other is the goal.
- **Generated `tauri.instance.json` lives in `$TMPDIR`, not the repo.** A crashed wrapper leaves the file behind;
  tracked space is sacred and `/tmp` self-prunes on macOS, so `.gitignore` needs no entry.
- **The dev-title worktree label rides a Vite `define`, not IPC.** For dev launches the wrapper exports
  `CMDR_WORKTREE_LABEL` (the `--worktree` slug, `"main"` for a `-m` main-clone run, or the worktree directory name for a
  plain dev launch from a worktree); `resolveWorktreeLabel` in `instance-id.js` is the pure resolver. `vite.config.js`
  bakes it into the `__CMDR_WORKTREE_LABEL__` compile-time constant (mirroring `__CMDR_I18N_CAPTURE__`), and
  `src/lib/app-mode.ts`'s `decorateMainWindowTitle` wraps it around the dev title bar, e.g.
  `(colorful-tags) DEV MODE - … - DEV MODE (colorful-tags)`. Skipped under E2E (so E2E titles stay unmarked by a label)
  and never set for prod. A dev-only cosmetic, so no Rust/IPC surface.
- **Ephemeral Vite + tauri-MCP ports are picked by the wrapper** via `net.createServer().listen(0)`, NOT by the
  consumers, because the wrapper knows the data dir and can write the port file BEFORE the consumer spawns. The race
  window (close → spawn → bind) is mitigated per-consumer: Vite uses `strictPort: true` so any collision is loud, and
  the Tauri-MCP plugin gets a post-bind connect-check on the Rust side that warns on mismatch.
