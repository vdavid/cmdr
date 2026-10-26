# Desktop scripts details

Depth and rationale. `CLAUDE.md` holds the must-knows; the wrapper architecture and decisions live here. The canonical
instance-isolation reference (per-resource derivation, race-window analysis, debug recipes, acceptance smoke) is
`docs/tooling/instance-isolation.md`.

## The wrapper architecture

`tauri-wrapper.ts` is the single composition point for dev vs prod. Pure helpers in `instance-id.ts` do the work so they
stay testable. For `pnpm dev`, the wrapper resolves an instance ID (from `--worktree <slug>`, the existing
`CMDR_INSTANCE_ID` env, or the default `"dev"`), reserves ephemeral ports, composes the bundle identifier, productName,
data dir, and generated config payload, writes the config to a `$TMPDIR/cmdr-tauri-instance-<rand>/tauri.instance.json`
(NOT in the repo, so a crashed wrapper can't pollute tracked space), writes the tauri-MCP port file BEFORE Tauri
launches (the plugin has no bound-port accessor, so external readers learn the port from the wrapper), and exports
`CMDR_DATA_DIR` and `CMDR_SECRET_STORE=file` for non-prod. Production leaves `CMDR_INSTANCE_ID` unset and runs
byte-identical to before instance isolation existed.

### Dev forces the MCP server on (`CMDR_MCP_ENABLED=1`)

For `pnpm dev` launches the wrapper exports `CMDR_MCP_ENABLED=1` unless the env var is already set. Without it, MCP is
silently dead across dev sessions: the FE persists `developer.mcpEnabled: false` (the registry default) as an explicit
saved value on first run, and that saved `false` then beats the debug-build-on default (`cfg!(debug_assertions)` in
`mcp/config.rs`) at every later launch. `mcp/config.rs::from_settings_and_env` reads `CMDR_MCP_ENABLED` ahead of the
setting, so the export wins; `CMDR_MCP_ENABLED=0` still exercises the off path. The deeper fix — not persisting registry
defaults as explicit choices — is a known settings-store follow-up (see `src-tauri/src/settings/DETAILS.md`).

### Dev answers the terminal question for the app (`CLICOLOR_FORCE=1`)

For `pnpm dev` launches, when the wrapper's own stderr is a TTY and neither `CLICOLOR_FORCE` nor `FORCE_COLOR` is
already set, the wrapper exports `CLICOLOR_FORCE=1`. The app's logger can't work this out for itself: the Tauri CLI
spawns the dev app with a piped stderr and forwards the bytes to its own, so `is_terminal()` in the app says false in a
real terminal and can't tell one from `2> log.txt`, and the terminal log would never colorize. The wrapper is the last
process in the chain that still sees the real stderr. Stderr, not stdout, because that's where the app's log lines come
out, so `pnpm dev > log.txt` keeps its colors while `pnpm dev 2> log.txt` doesn't.

The same move happens one layer down: the Tauri CLI hands cargo `--color always`. Dev only, because a build launches no
app, and forcing color on the tools a build runs would put escape sequences into a captured `pnpm build > build.log`.
How the app reads it (and `NO_COLOR` outranking it): `src-tauri/src/logging/DETAILS.md` § "Terminal target column".

## The llama-server fetch

`download-llama-server.go` runs from `src-tauri/build.rs` and puts the llama-server binaries where the build expects
them. In a linked worktree whose `.version` matches the main clone's, it takes them from there instead of the network:
an APFS clonefile (`cp -c`), falling back to a plain copy, and downloading only when neither is possible.

- **A copy, never a symlink into the main clone.** The Linux-E2E Docker container bind-mounts only the worktree, so a
  symlink pointing outside it dangles there and breaks the in-container build.
- **Signing is a separate subcommand, `sign`**, never part of the fetch. It codesigns each regular file (the binary and
  the dylibs, not their symlinks) with `APPLE_SIGNING_IDENTITY`, hardened runtime, and a secure timestamp, and refuses
  unless `.version` matches. The fetch runs inside every build (from `build.rs` and `beforeBuildCommand`), so it must
  never see the identity: the release workflow's `sign` job calls `sign` after a compile it doesn't run itself
  (`docs/guides/releasing.md` § How a release build is split). The bundler copies these files in as resources and
  doesn't sign them, so unsigned ones fail notarization.
- **`LLAMA_SIGN_KEYCHAIN`** makes `sign` pass `codesign --keychain` explicitly, and `release-pipeline.yml` ALSO puts
  that keychain in the search list: the runner's launchd session can't reach the login keychain's key, and `--keychain`
  on its own doesn't work for a keychain outside the search list.

## The capture binary

Neither capture orchestrator (`i18n-capture.ts`, `marketing-shots.ts`) builds or picks a binary itself.
`capture-runtime.ts`'s `ensureE2eBinary` asks the check runner (`scripts/check.sh --ensure-e2e-binary`), which reuses
the Playwright lane's stamped binary when it matches the tree, builds it otherwise, and prints its path. The build
command and the fingerprint inputs stay in Go (`scripts/check/checks/e2e-build.go`), so the capture and the lane can't
disagree about staleness, and no entry point (`i18n:shots`, `i18n:shots:no-couple`, `i18n:shots:overflow`,
`marketing:shots`) can launch an old binary.

One binary serves both runs. Every E2E build carries the capture instrumentation inert, and the launch turns it into a
capture with `CMDR_I18N_CAPTURE=1` beside `CMDR_E2E_MODE=1` (`src/lib/app-mode.ts`). The lane also runs the capture's
staging on that binary, with no camera and no `CMDR_I18N_CAPTURE`: `test/e2e-playwright/DETAILS.md` § "The i18n
capture's staging in the lane". The license passes need no build of their own either: `CMDR_MOCK_LICENSE` compiles into
`playwright-e2e` builds, and an E2E build never reaches a production server (`src-tauri/src/licensing/DETAILS.md`).

## The pre-capture countdown

Right before a capture spec starts driving the app, it fronts the Cmdr window and covers it in solid orange for three
seconds: a big "3, 2, 1" and a line like "This will take about 25 seconds." Whoever started the run is usually still
typing somewhere, and once Cmdr takes the front a stray letter, Tab, Enter, or Space would land in a pane. So while the
overlay is up, capture-phase listeners on `window` swallow keyboard and pointer input. Menu accelerators the OS handles
natively can slip through; accepted. Test-side DOM injection only, no app code:
`apps/desktop/test/e2e-playwright/capture-countdown-overlay.ts`, shared by `marketing-shots.spec.ts` (in its
`beforeEach`, fronted with `focusApp`) and `i18n-capture.spec.ts` (main pass only, fronted with `focusWindow`).

- **The orchestrator owns it.** `capture-countdown.ts`'s `startCaptureStopwatch` reads the stored estimate, words the
  line, and hands the spec `CMDR_CAPTURE_COUNTDOWN_S` / `CMDR_CAPTURE_COUNTDOWN_LINE`, plus
  `CMDR_CAPTURE_COUNTDOWN_ENDED_FILE`, where the spec stamps the moment the countdown ended. A lane run of the same spec
  sets none of them and never counts down.
- **It can't strand an orange window or dead input.** The spec removes the overlay and its listeners in a `finally`,
  then waits two animation frames so the first shutter can't catch it; and the page removes it by itself five seconds
  past the countdown even if the test process is gone.
- **The estimate** lives at `~/Library/Caches/com.veszelovszki.cmdr-capture/estimates.json`, per kind (`marketing`,
  `i18n`, `i18n-overflow`, `i18n-overflow-worst-case`; the overflow passes run only the main pass, so they'd skew the
  full run's figure). Machine-wide, so a fresh worktree doesn't restart from the seed; deleting it only costs one run
  with the seed. The stopwatch runs from that stamp to the green finish; a failed run, or one with no stamp, never
  calibrates. A run more than 20% off the stored figure replaces it outright (the next run looks like the last, so no
  averaging).
- **Why marketing counts down only after the index settles**: a cold index reconciles for about five minutes
  (`docs/guides/screenshots.md` § "When it waits"). The spec's `beforeEach` runs `waitForIndexedSizes` first, then the
  countdown, so "hands off" comes right before the shoot. ❌ Don't add an orchestrator-side index check instead: right
  after launch the old index reads `fresh` over MCP before the reconcile starts, so such a check passed at once and the
  countdown came five minutes early (2026-10-26). And because the clock starts at the countdown's end, no wait before it
  can teach the estimate the reconcile (that same run stored 324 s for a 25-second shoot).

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
`message-screenshots-fresh` can't tell a surface the app no longer shows from one it does, so nothing surfaces it.
Deleting the surface's object out of `capture-report.json` is safe and is what the next full run would do. Follow it
with `pnpm i18n:couple`, which moves each of its keys to the next surface it appeared on, or clears the coupling;
without that, the check fails on every `@key.screenshot` still naming the deleted PNG.

## The defaults manifest

`gen-analytics-defaults.ts` writes `apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json`: what a fresh
install runs with, per released app version, for exactly the keys the heartbeat's config shape can carry. The dashboard
joins it on `app_version` to tell "the user is on the default" apart from "the setting didn't exist in that build".

Three things about it are load-bearing:

- **The registry is PARSED, not imported.** `definitions/appearance.ts` pulls in `#lib/intl`, whose `messages.svelte.ts`
  is rune-compiled, so a bare `node` import can't evaluate the registry. Reading the definition objects off the
  TypeScript AST needs no Svelte toolchain, and it works against a `git show` of a release tag, which is what makes
  `--backfill` able to reconstruct the history from the tags.
- **`next` is not a version.** Between releases `package.json` still holds the LAST shipped version, so keying the
  working tree's snapshot by it would rewrite an entry that describes a release already in the field. `next` holds the
  unreleased state, is never resolved against, and `scripts/release.sh` promotes it under the real version number.
- **`promotedThrough` records every promote, entry or not.** `promote()` writes an entry only when a default moved, so a
  release that changed nothing leaves `versions` exactly as a skipped promotion would; the stamp is how
  `settings-defaults` tells them apart. Promote on a tree whose settings match the release: a later working tree records
  defaults that release never shipped. `--backfill` stamps the newest tag it could read.

The two locale generators emit `#[rustfmt::skip]`, so they own their layout and need no Rust toolchain.

## The frontend SBOM plugin

`vite-frontend-sbom.ts` is a Vite plugin `vite.config.js` loads only under `CMDR_FRONTEND_SBOM=1`, in the client build
only. It writes `build/sbom/frontend.cdx.json`: the npm packages whose code the bundle holds, plus the ones a plugin
inlines without a traceable import (today `@iconify-json/lucide`). The release workflow's `sbom` job sets the flag;
every other build leaves it unset, so the shipped app carries no SBOM. `docs/guides/releasing.md` § Provenance and SBOM
attestations.

## Key decisions

- **Pure helpers in `instance-id.ts`, side effects in `tauri-wrapper.ts`.** The sanitizer, identifier composer,
  port-file writer, and config-payload builder are all unit-testable in isolation. The wrapper is ~200 lines of obvious
  orchestration. Touching either side without breaking the other is the goal.
- **Generated `tauri.instance.json` lives in `$TMPDIR`, not the repo.** A crashed wrapper leaves the file behind;
  tracked space is sacred and `/tmp` self-prunes on macOS, so `.gitignore` needs no entry.
- **The dev-title worktree label rides a Vite `define`, not IPC.** For dev launches the wrapper exports
  `CMDR_WORKTREE_LABEL` (the `--worktree` slug, `"main"` for a `-m` main-clone run, or the worktree directory name for a
  plain dev launch from a worktree); `resolveWorktreeLabel` in `instance-id.js` is the pure resolver. `vite.config.js`
  bakes it into the `__CMDR_WORKTREE_LABEL__` compile-time constant (mirroring `__CMDR_E2E_BUILD__`), and
  `src/lib/app-mode.ts`'s `decorateMainWindowTitle` wraps it around the dev title bar, e.g.
  `(colorful-tags) DEV MODE - … - DEV MODE (colorful-tags)`. Skipped under E2E (so E2E titles stay unmarked by a label)
  and never set for prod. A dev-only cosmetic, so no Rust/IPC surface.
- **Ephemeral Vite + tauri-MCP ports are picked by the wrapper** via `net.createServer().listen(0)`, NOT by the
  consumers, because the wrapper knows the data dir and can write the port file BEFORE the consumer spawns. The race
  window (close → spawn → bind) is mitigated per-consumer: Vite uses `strictPort: true` so any collision is loud, and
  the Tauri-MCP plugin gets a post-bind connect-check on the Rust side that warns on mismatch.
