# Check authoring

Every check lives in this directory as a single Go file, registered in `registry.go`'s `AllChecks` slice. This file
covers everything you need to add or modify a check. For the runner architecture (parallel executor, dependency graph,
CLI flags, freestyle.sh remote execution), see [`../CLAUDE.md`](../CLAUDE.md).

## Key files

| File                                                 | Purpose                                                                                                                                                                                                          |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `common.go`                                          | Core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`), shared utils (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `runESLintCheck`, `indentOutput`, `trimBuildNoise`) |
| `registry.go`                                        | `AllChecks`: canonical ordered list of all check definitions. Lookup and validation functions (`FilterSlowChecks`, `FilterCIOnlyChecks`, `FilterFastChecks`, `ValidateCheckNames`).                              |
| `registry_test.go`                                   | Collision detection, `CLIName()` tests                                                                                                                                                                           |
| `desktop-rust-*.go`                                  | One file per Rust check                                                                                                                                                                                          |
| `desktop-svelte-*.go`                                | One file per Svelte/TS check                                                                                                                                                                                     |
| `website-*.go`, `api-server-*.go`, `scripts-go-*.go` | One file per check                                                                                                                                                                                               |
| `file-length.go`                                     | Informational file-length scanner (warn-only, never fails). Supports an allowlist.                                                                                                                               |
| `file-length-allowlist.json`                         | Allowlist for file-length check: `{ "files": { "path": lineCount } }`. Files at or below their allowlisted count are suppressed.                                                                                 |
| `changelog-commit-links.go`                          | Validates every `https://github.com/vdavid/cmdr/commit/<sha>` URL in `CHANGELOG.md` resolves, via a single `git cat-file --batch-check` process.                                                                 |

## Check definition shape

```go
CheckDefinition{
    ID:                "desktop-svelte-eslint", // unique, always accepted by --check
    Nickname:          "",                      // short alias, also accepted by --check (optional)
    DisplayName:       "eslint",                // shown in output
    App:               AppDesktop,
    Tech:              "🎨 Svelte",
    IsSlow:            false,
    IsFast:            false, // true = included in --fast (curated pre-commit lane)
    CIOnly:            false, // true = run only in --ci mode (or explicit --check)
    FreestyleIncompat: true,  // can NOT run on freestyle.sh VMs (Rust, Docker)
    CpuWeight:         2,      // avg cores busy; 0/unset = 1. Governs concurrent admission.
    DependsOn:         []string{"desktop-svelte-prettier"},
    Run:               RunDesktopESLint,
}
```

### Field semantics

- **`ID`** is the canonical name (`--check <id>` always works).
- **`Nickname`** is an optional short alias also accepted by `--check`. `CLIName()` returns nickname if set, else ID.
  `ValidateCheckNames()` runs at startup and fatals on any ID/nickname collision.
- **`IsSlow: true`** excludes the check from the default run; included by `--include-slow`/`--only-slow` or a named
  `--check`. Use for E2E suites, full eslint with type-aware rules, etc.
- **`IsFast: true`** opts the check into the curated `--fast` pre-commit lane. The lane is editorially picked, not
  derived from timings — only check this if the check is genuinely cheap on a warm cache _and_ unlikely to spike on a
  cold one.
- **`CIOnly: true`** runs the check only under `--ci` (or an explicit `--check`). Useful for the slow-but-authoritative
  variant of a check whose fast local variant lives elsewhere (e.g. `cargo-udeps` paired with `cargo-machete`).
- **`FreestyleIncompat: true`** opts out of freestyle.sh remote VM runs. Set for any Rust-compiling check or anything
  that needs Docker. Negative-sense default (`false` = compatible) keeps the field absent in the common case.
- **`DependsOn`** is a flat slice of IDs. Formatters before linters, linters before tests, type checkers before tests.
  Blocked checks (dep failed) get `StatusBlocked` automatically.
- **`CpuWeight`** is the average number of CPU cores the check keeps busy while running (cold/working profile, rounded).
  The runner admits checks so the sum of concurrent weights stays within `NumCPU`, so two CPU-heavy checks don't
  oversubscribe the machine. `0` (unset) counts as `1` (light). Weights are Docker-VM-aware (`rust-tests-linux` /
  `e2e-linux` burn cores in the VM the host process never shows). Calibrate from the isolation sweep in
  `docs/notes/check-cpu-contention.md`; visualize with `./scripts/check.sh --graph`. Only the measured non-fast checks
  carry explicit weights today; fast/formatters default to 1.

## Adding a new check

1. Create `{app}-{name}.go` with a `func RunSomething(ctx *CheckContext) (CheckResult, error)`. Use `website-build.go`
   or `website-docker.go` as templates; they're the simplest.
2. Register it in `AllChecks` in `registry.go` (ID, App, Tech, DependsOn, Run, plus any flag fields).
3. Return `Success("message")` on pass, `fmt.Errorf(...)` on fail, `Skipped("reason")` to skip.
4. Add a test file if the check has non-trivial logic (`{app}-{name}_test.go`).
5. Run `./scripts/check.sh --check go-vet --check staticcheck` to verify (staticcheck is strict about idiomatic Go).
6. Update the "Apps and check counts" table below and `AGENTS.md`'s `--check` list.

### Return values

- `Success(message)` on success with a short, informative message
- `Warning(message)` for non-fatal issues
- `Skipped(reason)` when the check can't run (for example, missing config)
- `CheckResult{}, error` on failure
- `SuccessWithChanges(message)` when the check made local fixes (auto-fix mode); CI mode should still error

### Success messages

Include useful stats: "12 tests passed", "Checked 42 files", "No lint errors". Avoid generic "OK".

### Error messages

Include the command output using `indentOutput()`:

```go
return CheckResult{}, fmt.Errorf("check failed\n%s", indentOutput(output))
```

### Length-based truncation is forbidden

If 200 tests fail, all 200 panic bodies must pass through. Filter by structure (section delimiters, line-anchored
regexes for harness noise), never by max-line count. See the "E2E failure output" and "cargo test output" decisions
below for the section-aware patterns to follow.

## Common helpers

- **`RunCommand(ctx, name, args...)`** — wraps `exec.Cmd` with the runner's working dir, captured output, and timeout
  hooks.
- **`CommandExists(name)`** — checks `PATH` before invoking.
- **`EnsureGoTool(name, installPath)`** — checks `PATH` first, then `go install`s; returns the full binary path. Used
  for staticcheck, nilaway, etc. `installPath` MUST pin a specific version (`@vX.Y.Z` or a pseudo-version), never
  `@latest`. Same rule applies to `cargo install` calls inside checks: pin both `--version` and `--locked`.
- **`runPrettierCheck(ctx, ...)`** / **`runESLintCheck(ctx, ...)`** — auto-fix locally, check-only under `--ci`.
  Centralizes the dual-mode behavior so individual checks don't reinvent it.
- **`indentOutput(s)`** — indents captured stdout/stderr for error messages.
- **`trimBuildNoise(s)`** — cuts everything before the last `Compiling …` line; when no such line exists (build failed
  before cargo ran), returns input verbatim.

## File-length allowlist

`file-length-allowlist.json` maps relative paths to accepted line counts:

```json
{
  "files": {
    "apps/desktop/src/lib/foo/bar.ts": 412
  }
}
```

- Files at or below their allowlisted count are silently suppressed.
- Files that grow beyond their allowlisted count are reported with both the current and allowed line counts.
- New files not in the allowlist are reported normally.
- When the allowlist suppresses all long files, the check shows "No new long files (N allowlisted)".
- If the allowlist file is missing, all long files are reported (backwards-compatible).

See `.claude/rules/file-length-allowlist.md` (repo-level) for when an entry may be raised vs lowered without user
consent.

## Apps and check counts

| App        | Tech     | Checks                                                                                                                                                                                                                                 |
| ---------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop    | Rust     | rustfmt, clippy, cargo-audit, cargo-deny, cargo-machete, cargo-udeps (CI-only), jscpd, log-error-macro, error-string-match, lock-poison, bindings-fresh, ipc-enum-camelcase, tests, integration-tests (Docker SMB), tests-linux (slow) |
| Desktop    | Svelte   | prettier, eslint, eslint-typecheck (slow), stylelint, css-unused, a11y-contrast, btn-restyle, bare-poll, svelte-check, import-cycles, knip, type-drift, tests, e2e-linux-typecheck, e2e-linux (slow), e2e-playwright (slow)            |
| Website    | Astro    | prettier, eslint, typecheck, build, html-validate, e2e                                                                                                                                                                                 |
| Website    | Docker   | docker-build                                                                                                                                                                                                                           |
| API server | TS       | oxfmt, eslint, typecheck, tests                                                                                                                                                                                                        |
| Scripts    | Go       | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests, govulncheck                                                                                                                                   |
| Other      | Metrics  | file-length (warn-only), CLAUDE.md-reminder (warn-only), changelog-commit-links, workflows-rustup (forbids `rustup target/component add` in workflows)                                                                                 |
| Other      | Security | workflows-hardening (SHA-pinning, no `pull_request_target`, job-scoped `id-token: write`)                                                                                                                                              |

## Key decisions

**Decision**: every operational `cargo` command in checks passes `--locked`. **Why**: without it, cargo silently
re-resolves `Cargo.lock` whenever upstream metadata shifts (a yank, a new transitive dep version). For a 1028-crate
lockfile, that resolution window is wide and lets a freshly-published malicious version land mid-build. `--locked` fails
loudly if the lockfile would change. Applies to `cargo clippy`, `cargo nextest run` (in both `desktop-rust-tests` and
`desktop-rust-integration-tests`), and `cargo +nightly udeps`. Audit/deny/machete read `Cargo.lock` without updating it,
so `--locked` is moot for them, but the install of those tools still uses `--locked` to lock the tool's own dep tree.

**Decision**: every tool install pins `--version` and `--locked` (cargo) or `@vX.Y.Z` (Go). **Why**: an unpinned tool
install (`cargo install cargo-audit` or `EnsureGoTool(..., "@latest")`) means each fresh checkout pulls whatever's
latest. A wave-1-2-class compromise of any of these tool repositories would auto-propagate. Pinning is the Go-side
equivalent of the pnpm `minimum-release-age` defense (a fresh version can't land without a deliberate bump).

**Decision**: `cargo-deny` advisories check disabled; use `cargo-audit` instead. **Why**: Tauri's transitive
dependencies (gtk3-rs, unic-\*, fxhash, proc-macro-error, etc.) trigger unmaintained-crate advisories we can't control.
`cargo-audit` still catches critical security vulnerabilities. License, bans, and sources checks in `cargo-deny` remain
active. See comment in `src-tauri/deny.toml`.

**Decision**: `workflows-hardening` check enforces three GitHub Actions invariants and acts as a regression guard.
**Why**: cmdr's workflows are already correctly hardened (every third-party action is SHA-pinned with a comment, no
`pull_request_target` triggers, no workflow-scoped `id-token: write`). Without an automated guard, a future PR or a
Renovate misconfiguration could silently regress any of those without anyone noticing in review. The check fails on
tag/branch-pinned third-party actions, on `pull_request_target` triggers (wave-4's entry vector), and on workflow-scoped
`id-token: write` (must be job-scoped per the wave-4 OIDC-token-extraction lesson). Local actions (`./...`) are exempt.

**Decision**: `govulncheck` runs against every Go module. **Why**: cargo-audit covers Rust deps; nothing covered Go
until now. `govulncheck` is static-analysis-based, so it only flags vulns actually reachable from the code (low false
positive rate). Most of cmdr's Go modules are dep-free tooling scripts but still call into the Go stdlib, which gets its
own CVEs; the check found 7 real reachable stdlib vulns the first time it ran (fixed by bumping mise's Go pin). Mirrors
the cargo-audit role on the Rust side.

**Decision**: `cfg-gate` check to catch ungated macOS-only crate imports. **Why**: Rust code using macOS-only crates
(from `[target.'cfg(target_os = "macos")'.dependencies]`) compiles fine on macOS but fails on Linux if the `use` isn't
wrapped in `#[cfg(target_os = "macos")]`. CI catches this after push, but the check catches it locally and instantly. It
parses `Cargo.toml` for macOS-only crate names, detects module-level gating (for example,
`#[cfg(target_os = "macos")] mod foo;` in `lib.rs` makes everything inside `foo` inherently safe), and scans remaining
files for ungated `use` statements.

**Decision**: `bare-poll` check to catch silently-passing E2E tests. **Why**: Cmdr's `pollUntil` helper (and its
wrappers `pollFs`, `pollUntilValue`, `pollActiveMode`, `pollOverlayGone`, `pollFocusedPane`) returns `false` on timeout
instead of throwing. A bare `await pollUntil(...)` statement therefore reduces to "wait up to N seconds, then quietly
proceed" — if the polled condition never holds, the test passes green so long as no later `expect` happens to catch it.
A repo-wide grep turned up 187 bare-poll sites across 20 specs; several spec files contained tests with zero `expect()`
calls whose entire assertion was a bare `await pollUntil(...)` — those tests literally couldn't fail. The check is a
fast-lane Go scanner (`apps/desktop/test/`, ~9 ms warm) modeled on `error-string-match`. Same line-anchored grep
pattern: `^\s*await\s+(pollUntil|pollFs|…)\s*\(` only matches the bare-expression-statement shape, so
`expect(await pollUntil(…)).toBe(true)` / `if (!(await pollUntil(…)))` / `return await pollUntil(…)` /
`const ok = await pollUntil(…)` all pass through. Opt out for genuine best-effort cleanups (dismissing an overlay that
might or might not be there) with `// allowed-bare-poll: <reason>` on the line above or as a trailing comment. The
preferred migration target is Playwright's `expect.poll(() => …).toBeTruthy()`, which fuses the wait with the assertion
so the bug class is structurally impossible.

**Decision**: `lock-poison` check to force a deliberate poison-handling choice at every std-lock acquisition. **Why**: A
bare `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` aborts the whole app when the lock is poisoned (a
background thread panicked while holding it), and records no intent — a reader can't tell a considered abort from a
thoughtless one. The policy (recover-by-default for value stores via `lock_ignore_poison()`; abort only for
invariant-guarding locks, marked by an `.expect("… poison …")` whose message names poison) lives in the module doc of
`apps/desktop/src-tauri/src/ignore_poison.rs`. The check is a fast-lane Go scanner (`apps/desktop/src-tauri/src/`,
modeled on `error-string-match`) that flags bare unwraps and non-poison `.expect(…)`. Its matcher requires empty parens
(`.lock()` / `.read()` / `.write()` with nothing between) immediately followed by `.unwrap()` / `.expect(`, so
`io::Read::read(&mut buf).unwrap()`, `io::Write::write(buf).unwrap()`, and tokio's `mutex.lock().await` all pass
through; `try_lock` / `try_read` / `try_write` are out of scope by name. Opt out with `// allowed-lock-poison: <reason>`
on the line above or as a trailing comment. Unlike `error-string-match`, it skips in-file `#[cfg(test)]` mods (tracked
by brace depth): a poisoned lock in a test means the test already panicked, so aborting there is harmless.

**Decision**: Split `desktop-svelte-eslint` into fast (non-type-aware) and slow (full) checks. **Why**: Type-aware rules
(`no-floating-promises`, `no-unsafe-*`, etc.) take ~45% of lint time due to TypeScript project service startup. The fast
check sets `ESLINT_NO_TYPECHECK=1`, which `eslint.config.js` reads to use `tseslint.configs.strict` (no type info) and
suppress `reportUnusedDisableDirectives` (since disable comments for type-aware rules would look unused). The slow check
(`IsSlow: true`) runs the full config with all rules and `reportUnusedDisableDirectives` on, so stale disable comments
are still caught.

**Decision**: Clippy runs the enforcing pass (`-D warnings`) first, and only invokes `cargo clippy --fix` if that fails
(and we're not in CI). **Why**: Running `--fix` speculatively before every check doubled wall time on the happy path (no
warnings = no fix to apply). The enforcing pass is the one that actually decides pass/fail anyway, and `--fix` ignores
`-D warnings` so it can't be combined into a single invocation. The trade is one extra re-check on the rare warning
path; cuts ~50% off clippy in the common clean case.

**Decision**: `bindings-fresh` is hash-cached. **Why**: A naive `pnpm bindings:regen` on every invocation takes ~2 min
for a test-mode compile of the full crate just to confirm the output didn't change. Instead, we hash every `.rs` file
under `src-tauri/src` plus `Cargo.lock` and `Cargo.toml`, plus the current `bindings.ts`, and store both hashes in
`<CARGO_TARGET_DIR or workspace target>/.bindings-fresh-marker` after each successful run. If both match next time, we
return OK in <100 ms. The bindings.ts hash in the marker protects against manual edits; if someone hand-tweaks the file
the marker no longer matches and we run the full regen. The marker lives inside cargo's actual target dir (honoring
`CARGO_TARGET_DIR`), so `cargo clean` and wholesale `target/` deletion auto-invalidate it. Same shared-fate pattern as
`node_modules/.pnpm-install-marker`. Hashing all `.rs` files (rather than only those with `#[tauri::command]` /
`specta::Type`) costs ~tens of ms here and removes any "added the attr to a new file but the watch list didn't pick it
up" footgun.

**Decision**: `bindings-fresh` auto-regens outside `--ci`. **Why**: When the hash check detects drift, the check runs
`pnpm bindings:regen` and, in non-CI mode, keeps the regenerated `bindings.ts` (returns `SuccessWithChanges`). Same
philosophy as `oxfmt`, `gofmt`, and `clippy --fix`: locally fix what's mechanical so the dev reviews and commits the
diff alongside the Rust change that caused it, instead of being told to run a separate command and re-run the checker.
In `--ci` mode the regenerated output is byte-compared against the original (which is then restored, so CI never
modifies the tree) and the check fails on drift, with a "Run `pnpm bindings:regen` from `apps/desktop/`" hint. The
marker is updated either way so the next run short-circuits.

**Decision**: `cargo-machete` runs locally; `cargo-udeps` is CI-only. **Why**: Both detect unused dependencies but trade
off speed against precision. udeps compiles the whole crate with nightly (~2 min cold) and is authoritative. machete
greps source files for `use foo;` patterns (~0.5 s on this codebase, no compile) and catches the common case (removed
the last `use` but forgot to drop the dep) plus a class udeps misses ("transitively-used" deps where your Cargo.toml
lists serde but only a transitive dep actually uses it). machete's blind spot is deps used only inside macro expansions
or build.rs codegen; opt those out via `[package.metadata.cargo-machete] ignored = ["foo"]` in the relevant Cargo.toml.
Local dev gets instant feedback from machete; CI runs udeps for the long-tail check.

**Decision**: E2E failure output uses section-aware filtering, not a pattern denylist. **Why**: The checker's contract
with agents is that output is concise enough to read in full: no `head`/`tail`/`grep` needed. Raw Playwright + Tauri +
Docker output is 1000+ lines on a failure (test pass markers, app stdout log, post-ELIFECYCLE build dump). The captured
output has four stable sections (setup, per-test progress, numbered failure blocks, post-ELIFECYCLE dump), split by
fixed delimiters (`Starting Tauri app...`, `\d+\) \[tauri\]`, `[ELIFECYCLE]`). `extractE2ETestOutput` in
`desktop-svelte-e2e-playwright.go` keeps the failure blocks verbatim, drops the post-ELIFECYCLE dump, and in the
progress section keeps `✘` markers with their preceding annotation lines (like `[SMB diag] MCP port: …`) while dropping
`✓`/`-` markers with theirs. The untouched output stays in the timestamped log file the error message links to. Both
`desktop-e2e-linux` and `desktop-e2e-playwright` call the same helper.

If the run died before reaching the test phase, none of `Starting Tauri app...`, a `\d+) [tauri]` failure-block header,
or a `\d+ (passed|failed|flaky|skipped)` tally line will be present. `isPreTestFailure` checks all three; only if all
three are absent does the filter prepend `note: tests did not reach the run phase` and drop the verbose
`docker compose ps` table (anchored on its `NAME IMAGE COMMAND` header so prose containing `Up <N>` survives). Checking
the tally and failure block (not just the Tauri marker) avoids false positives on macOS playwright shards, where Tauri
is started by the Go check and its stdout goes to a per-shard log file, so the marker never appears in Playwright stdout
regardless of success.

**Decision**: `cargo test` / `cargo nextest` failure output is filtered by dropping pass/skip verdict lines only.
**Why**: A 1786-test run produces ~1800 noise lines around 2 real failures. The harness format is stable enough that a
single per-line regex (`^test … ... (ok|ignored…)$` for `cargo test`, `^\s+(PASS|SKIP) [...] …$` for `cargo nextest`)
can drop the noise without risking false positives on panic-message bodies (start-of-line anchor protects quoted test
phrases). `trimRustTestProgress` in `desktop-rust-tests-linux.go` runs after `trimBuildNoise`. Everything else
(`running N tests` header, FAIL/FAILED/LEAK/TIMEOUT verdicts, the `failures:` block, the `test result:` / `Summary`
tally, `error:` lines, bench results) passes through unchanged.

**Decision**: nextest binary is arch-aware. **Why**: `https://get.nexte.st/latest/linux` serves the x86_64-musl build by
default; on an arm64 container (e.g. Apple Silicon under OrbStack) cargo's rustup-shim happily syncs the aarch64
toolchain, then execs the x86 nextest binary and OrbStack crashes with
`Dynamic loader not found: /lib64/ld-linux-x86-64.so.2`. The fix is `dpkg --print-architecture` → `linux` for amd64 and
`linux-arm` for arm64, matching the Go tarball selection.

**Decision**: silence apt/dpkg at the source, not via a post-hoc denylist. **Why**: `provisionScript` redirects both apt
commands to a log file under `DEBIAN_FRONTEND=noninteractive` + `-qq`, so on a successful provision the check's stdout
gets zero apt lines. The log lives on a per-run host directory (`/tmp/cmdr-rust-tests-linux-<unix-ts>/provision.log`)
bind-mounted into the container at `/cmdr-logs`, so it survives the container's `--rm` and is discoverable from the
check's Success/failure message. On apt failure, the script dumps the full log to stderr (captured by Go) so the user
sees what went wrong without having to fish for the file. Redirection at source is bulletproof and zero-maint vs a
denylist treadmill: every Debian version adds new dpkg verbs (`Setting up`, `Unpacking`, `Processing triggers`, …),
continuation lines from multi-line apt prompts have no stable shape, and `apt-get -qq` alone doesn't propagate to dpkg's
per-package chatter. `trimBuildNoise` now only cuts everything before the last `Compiling …` line; when no such line
exists (provisioning died before cargo ran), the output is returned verbatim.
