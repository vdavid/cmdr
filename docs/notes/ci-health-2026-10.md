# CI health, 2026-07-07 to 2026-10-06

`main` was red a lot over the summer (#118: "192 of 416 runs on `main` failed in three months"). It's much less frequent
now: a run of fixes since late September took the red rate from about half of all pushes to one in seven, and the
changes made on 2026-10-06 target what was still getting through.

## Where it stands

- **October 1–6: 6 of 40 pushes red (15%)**, against 134 of 255 (53%) in September and 44% over the three months.
- **What drove it down**, in order:
  - **Website renderer pinned (2026-09-08).** The Website job runs in the Playwright container its visual baselines are
    shot in, which ended renderer-drift pixel diffs.
  - **Git hooks (2026-09-30, `docs/tooling/git-hooks.md`).** Commit and push format with the same tools CI checks.
    Formatting was 63 red runs before them; one since.
  - **Flake fixes (by 2026-09-30).** The four recurring flaky Rust tests listed below were fixed; none has failed since.
  - **`clippy-linux` (2026-09-30), now with rustdoc (2026-10-06).** Lints and doc-checks the Linux target from a Mac in
    Docker. It stays an opt-in slow lane (David's call), so it catches Linux-only breakage for whoever runs it before
    pushing. The rustdoc half covers the doc links that went red three times in October alone.
  - **Pre-push notices and size limits (2026-10-06).** A push that moves a lockfile regenerates
    `THIRD-PARTY-NOTICES.md`, and every push checks `file-length` and `claude-md-length` before CI does
    (`docs/tooling/git-hooks.md`).
  - **macOS lane (2026-10-06).** `desktop-rust-macos` (`docs/tooling/ci.md` § macOS lane) compiles, lints, and tests the
    macOS-only code no CI job used to touch. Non-blocking until two clean weeks (issue #368).
- **October's six reds, and what covers each now:** rustdoc links to macOS-only items, three (`36919304043`,
  `37374925748`, `37377933557`: `clippy-linux`); stale notices, one (`36953758363`: pre-push); an over-budget
  `CLAUDE.md` plus a Linux-only dead function, one (`37425018587`: pre-push for the first, `clippy-linux` for the
  second); an ESLint error, one (`37103401131`: the default local lane already runs ESLint, so this one only needed
  running it).

## History: why it was red (2026-07-07 to 2026-10-06)

Measured from the runs rather than assumed. Method: `gh run list --branch main --created '>=2026-07-06'` (678 runs, all
workflows), then `gh run view --json jobs` and `--log-failed` for every failed or cancelled CI run (244), classified by
the check runner's `To rerun the failed check: pnpm check <name>` line, the Playwright summary, and the cargo
diagnostics.

### Headline numbers

- **CI on push to `main`: 528 runs, 232 red (44%).** One run per push (GitHub runs only the pushed head), so this is
  also the share of pushes that ended red. Per month: July 31/51, August 61/183, September 134/255, October 1–6 6/40.
- **Time red: 635 of 2,173 hours (29%)**, across 91 red streaks. Median streak 1.9 h, 43 streaks lasted one push, five
  lasted over 24 h. The longest by run count (11 each): 2026-07-11..18 (`29172297511` onward: clippy's `allow` without
  reason in `mcp/executor`), 2026-09-08..09 (`34284084416`), 2026-09-22..23 (`35789523407`: rustdoc
  `find_mounted_share`).
- **Infra: zero.** No failed run carries a runner, disk, cache, network, or rate-limit signature. The nine cancelled
  runs were manual cancels. The only infra-shaped finding is latent: the Actions cache sits at 11.6 GB against GitHub's
  10 GB ceiling, with three ~1.9 GB `Linux-docker-e2e-<Cargo.lock hash>` entries alive at once.
- **Flakes: about 10 runs (4%)**, all fixed by 2026-09-30: `cold_drive_tests::removals` (`35609549095`, `36345550757`,
  `36637760523`, fixed in `871f1494f`), `git::wiring_tests` debounce (`33996224584`, `33998657552`, `34001957865`), an
  `importance::scheduler` stop-test timeout (`36729988265`), and two `svelte-tests` coverage dips. Playwright retries
  absorb the E2E flakes (`rename-chaining` passed on retry in five red runs), so they rarely decide a run.
- **Everything else is real breakage that a local `pnpm check` on a Mac doesn't see.**

### What failed, by cause (runs; a run can hit several)

1. **Linux-only Rust breakage: 79 runs** (48 with no other cause). Every local lane compiles for macOS, so code under
   `cfg(target_os = "linux")`, and code only macOS uses (dead on Linux), reaches CI unchecked.
   - `clippy`, 52 runs: almost all dead code or unused imports on Linux (`ClipTower` `33075955678`,
     `ICLOUD_DRIVE_SUBPATH` `34143223152`, `local_path_of` `37425018587`), plus lints inside `volumes_linux/`
     (`36759764170`).
   - `rustdoc`, 27 runs: mostly intra-doc links to macOS-gated items (`query_task_vm_info` `30624996082`,
     `apple_languages` `32293336601`, `find_mounted_share` 7 runs from `35789523407`).
   - The Linux E2E build, 24 runs: the same compile errors stop the app build (`native_drag` `33977734585`).
   - Mitigation since 2026-09-30: the `clippy-linux` slow lane (now also rustdoc, see § Where it stands). It's opt-in,
     so six of these still landed after it existed.
2. **Formatting: 63 runs** (`oxfmt` 62, `rustfmt` 3). 115 of the 135 flagged files were Markdown: doc-only commits that
   ran no checks. The git hooks (2026-09-30, `docs/tooling/git-hooks.md`) fixed it: one `oxfmt` red since.
3. **Stale artefacts, budgets, and static scanners: 60 runs.** `third-party-notices` 26 (a lockfile change pushed
   without regenerating `THIRD-PARTY-NOTICES.md`, still recurring: `36953758363` on 2026-10-02), `file-length` 7 and
   `claude-md-length` 1 (a file crossing its limit inside a multi-commit push: `routing.rs` at 805 lines,
   `36693215630`), `pluralize-noun` 6, `knip` 5, and 18 one-off scanner hits.
4. **Linux E2E test failures: 54 runs**, nearly all sustained regressions rather than flakes:
   - a search/indexing cluster of ~10 tests (`search-*`, `indexing.spec.ts`, `mcp-indexing.spec.ts`), 14–18 runs each
     from 2026-08-23 to 09-23, 22-minute runs (`32631987280`);
   - type-mismatch conflicts, seven runs 2026-08-26..09-06; SMB share listing, four runs on 2026-09-30 (`36663621908`);
   - three mass failures where the app wedged (223 of 309 failed over 2.0 h, `32915058946`, before the job timeout
     existed; 84 failed, `34400129266`; 243 failed, `35826304650`).
5. **Unit and integration tests: 34 runs** (`svelte-tests` 15, `rust-tests` 14, Docker fixtures 5). Mostly real (a
   missing `vi.mock` export `35242300251`, a below-threshold coverage file `32654600130`) plus the flakes above.
6. **Website visual baselines: 12 runs**, all `visual.spec.ts` pixel diffs; renderer drift ended when the job moved into
   the pinned Playwright container (2026-09-08). A handful more were the move itself (`sh` vs `bash` in the Lighthouse
   step, git's safe-directory guard).

Noise worth knowing: in 23 red E2E runs the "Upload E2E screenshots" step failed too (`if-no-files-found: error` when
the build died before any test ran), adding a second red step with no new information.

## Calls made (2026-10-06)

- **Declined: run the Linux lanes by default.** Cause 1 was the biggest, and `clippy-linux` costs ~25 s warm (measured
  2026-10-06, 13 crates) but needs Docker. David kept it a slow, opt-in check.
- **Open: promote `desktop-rust-macos` to required** after two weeks of clean runs (issue #368).
