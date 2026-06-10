# Maintenance

This file tracks the recurring maintenance work for the Cmdr monorepo: what tasks repeat, on what cadence, and a log of
past runs.

The goal isn't to be a calendar: it's to make the repeat work explicit so nothing quietly slips, and to give us a paper
trail when we wonder "when did we last bump Rust crates?". Later, we'll likely turn this into a Go script that emails
reminders or wakes up an agent.

The log lives inside a fenced code block as tab-separated lines on purpose: stays grep-friendly, parses cleanly when we
automate it, and survives `oxfmt` (which collapses whitespace in regular markdown).

## Tasks

### Dependency upgrades

- **Bump npm packages**: Bump everything in `pnpm-lock.yaml` across all apps (desktop, api-server, website,
  analytics-dashboard) to latest minus ~3-4 weeks to dodge zero-day vulns. Run `pnpm dedupe` after, then the full check
  suite. Frequency: monthly, on the 10th.
- **Bump Rust crates**: Bump `Cargo.toml` and `Cargo.lock` across the workspace. Patch + minor first; majors in a
  separate commit so each migration is reviewable. Frequency: monthly, batched with the npm sweep.
- **Bump Go modules**: Run `go get -u ./... && go mod tidy` in `scripts/check/`. Tiny but worth tracking. Frequency:
  monthly, batched with the npm/Rust sweep.
- **Bump pinned tool versions in `.mise.toml`**: Node, Go, Rust toolchain, pnpm itself. These are pinned in `.mise.toml`
  and need explicit bumps. Frequency: quarterly, or when a security CVE on a toolchain forces it.
- **Bump GitHub Actions**: Update `actions/checkout`, `actions/cache`, `jdx/mise-action`, etc. to the latest Node
  runtime ahead of GitHub's deprecation deadlines. Frequency: roughly every 6 months, driven by the Node-runtime
  deprecation cycle.
- **Security advisory sweep**: Drain whatever `cargo audit` and `pnpm audit` report. Renovate handles most of this
  automatically now, but check anyway in case advisories slipped past it. Frequency: monthly with the dep sweep, plus
  immediately for any high-severity advisory.
- **Review `cargo-audit` ignores**: `apps/desktop/src-tauri/.cargo/audit.toml` carries ignored advisories that
  cargo-audit can't verify as still-needed (currently `RUSTSEC-2023-0071`, waiting on `sspi` updating its `rsa` dep).
  For each entry, check whether the upstream fix landed; if it did, remove the ignore. Frequency: quarterly, batched
  with the allowlist cutback.

### Codebase health

- **Split files that grew too big**: Walk the file-length warnings, pick the worst offenders, and split them into
  focused modules. Frequency: quarterly, or whenever the warning list gets noisy.
- **Cut back allowlists (the non-automated rest)**: Most allowlist staleness is now caught automatically — the
  file-length check shrink-wraps its own allowlist on local runs; svelte-tests removes dead coverage-allowlist entries
  and warns on satisfied ones; a11y-coverage fails on dead/redundant entries; the comment scanners (`bare-poll`,
  `lock-poison`, `error-string-match`, `btn-restyle`, `workflows-rustup`) fail on orphaned opt-out comments; knip treats
  unused ignores as errors; stylelint reports needless disables; cargo-deny denies unused allowed licenses. What's left
  for the manual pass: Clippy `#[allow]` attributes, ESLint disable comments outside the type-aware lane, and judging
  the "coverage satisfied" warnings. Frequency: quarterly.
- **Dead code sweep**: Hunt for un-wired features, stale `#[allow(dead_code)]`, unused exports, tautological tests,
  mock-only tests. Frequency: quarterly.
- **AI-smell / inelegance review**: Ask a fresh agent (different from any that wrote the code) to review the codebase
  for inelegant patterns, copy-paste duplication, AI-flavored verbosity, name-restating doc comments, and
  over-abstracted helpers. Then fix what comes up. Frequency: quarterly.
- **Review file structure**: Step back, look at the directory tree, and ask whether things still live in the right
  places after the last quarter of growth. Reorganize where it earns its keep. Frequency: quarterly, or after a major
  feature lands.
- **Replace AI-generated content with handcrafted**: Walk user-facing surfaces (icons, marketing copy, privacy policy,
  error messages) and replace anything that smells like AI slop. Frequency: ad hoc, but check at least quarterly.

### Documentation hygiene

- **Purge old specs**: Delete or archive `docs/specs/` files for features that have shipped. Migrate any still-useful
  intent into colocated `CLAUDE.md` files. Frequency: quarterly, or right after a feature ships.
- **Refresh `CLAUDE.md` files**: Walk the colocated `CLAUDE.md` files, fix anything stale, fill in gaps where new
  modules are missing one. Frequency: quarterly.
- **Refresh `AGENTS.md`**: Slim it down where it grew, clarify where I've been corrected often, prune stale rules.
  Frequency: quarterly.

### Tooling discipline

- **Tighten linter rules**: Add new Clippy denies, ESLint rules, Stylelint rules where the codebase is ready for them.
  Each tightening means fixing existing violations in the same pass. Frequency: ad hoc, when code reviews surface a
  pattern worth ruling on.
- **Pin GitHub Actions to commit SHAs**: Re-pin Actions to fresh commit SHAs when their tagged versions move.
  Supply-chain safety. Frequency: every 6 months alongside the Actions version bump.

### Test-suite health

- **Quarterly: mutation-testing sweep on hot-spot modules**. Run `cargo mutants` on each module listed in
  `docs/testing.md` § "Hot spots" (write_operations, indexing, file_viewer, file_system/index/store). Triage survivors.
  If mutation score drops below 80% on any of them, add tests to bring it back. The pass is slow (~10–15 min per file on
  this hardware); budget half a day. Document the run + score in the log below.
- **Per release: E2E suite health check**. Three back-to-back `pnpm check desktop-e2e-playwright` runs. All three must
  be green (no flakes). Look at the slowest 5 tests; if any have crept back to `sleep()`-based waits (the lint catches
  new ones, but existing `eslint-disable` annotations may need re-triaging), replace with `pollUntil`. Document the
  run + flake rate + slowest tests in the log.
- **Per release: scan for new fixed sleeps in E2E specs**. Run
  `grep -rE "await sleep\(" apps/desktop/test/e2e-playwright/*.spec.ts | grep -v "eslint-disable"` (should return
  empty). If not, the new sleeps got past the lint somehow and need attention.
- **As needed: surviving `eslint-disable cmdr/no-arbitrary-sleep-in-e2e` annotations**. ~66 of these were added during
  the Step 6 speedup as a legacy migration tool. Each is a candidate for replacement with `pollUntil`. Picking off a few
  per quarter shrinks the technical-debt surface without forcing a single huge cleanup.

## Log

Newest-first. Tab-separated columns: `date`, `hash(es)`, `task`, `comment`.

```log
2026-05-10	cee0aa08	Bump pinned tool versions in .mise.toml	Pinned pnpm 11.0.9 in .mise.toml and regenerated lockfile after CI lockfile drift; migrated allowBuilds config for pnpm 11.
2026-05-09	51dff4c1	Split files that grew too big	Split friendly_error.rs (1410 lines) into a 5-file module directory.
2026-05-09	64eb64dc	Cut back the file-length allowlist	Dropped stale friendly_error.rs entry (file became a directory after the split); clarified the allowlist rule in .claude/rules/.
2026-05-08	c002c7ef	Bump pinned tool versions in .mise.toml	Dropped redundant packageManager field from package.json; .mise.toml is now sole source of truth for the pnpm version.
2026-05-06	2f02fa7e	Bump GitHub Actions	actions/cache v4→v5.0.4 and dorny/paths-filter v3→v4.0.1 to Node 24 runtimes ahead of GitHub's 2026-06-02 Node 20 deprecation deadline.
2026-05-06	a6b4613a	Dead code sweep	Removed un-wired and stale #[allow(dead_code)] items across 22 files; deleted 355 lines: 4 stale rationales corrected, 3 un-wired features deleted, 14 genuinely dead utilities.
2026-04-27	7bb7972a	Bump Rust crates	Major Rust deps: sha2 0.10→0.11, rand 0.9→0.10, zip 4.6→8.6 (each required a small migration); bundled astro 5→6 in the same commit.
2026-04-27	ed6fff03	Bump Rust crates	Risky deps verified one by one: mdns-sd 0.19, mtp-rs 0.13.2, vite 8 (analytics), eslint 10 (website), satori 0.26, marked 18, lhci 0.15.
2026-04-27	2036e835	Bump npm packages, Bump Rust crates, Bump Go modules	Patch+minor sweep across npm/Rust/Go in one commit: pnpm 10.32→10.33, oxfmt 0.42→0.46, Playwright 1.58→1.59, Svelte 5.54→5.55, Vitest 4.0→4.1, plus ~25 others; Rust workspace patch bumps.
2026-04-27	c0f7a747	Bump npm packages	TypeScript 5.9→6.0 across all four apps; typescript-eslint 8.59 supports TS 6.0.
2026-04-26	b1a53acb	Bump npm packages	Bumped 24 npm deps + collateral fixes; pnpm dedupe collapsed duplicate postcss and @playwright/test, fixing stylelint and typecheck false positives.
2026-04-23	9ff7583d	Security advisory sweep	rustls-webpki 0.103.12→0.103.13 to clear RUSTSEC-2026-0104 (reachable panic in CRL parsing).
2026-04-15	3734502a	Security advisory sweep	rustls-webpki 0.103.11→0.103.12 (RUSTSEC-2026-0098/0099) and bitstream-io 4.9.0→4.10.0 (drops yanked core2); compacted cargo-audit output via JSON in same pass.
2026-04-14	315609a3	Split files that grew too big	Split four large files: friendly_error.rs (1805) → provider.rs; ai/manager.rs → system_memory.rs; AiSection.svelte (1582) → AiCloud/AiLocal; +page.svelte (1222→750) → command-dispatch + mcp-listeners + explorer-api.
2026-04-14	2939bfee	Split files that grew too big	Split eight more files into sub-800-line modules: volume_copy.rs, scan.rs, smb.rs, stress_tests.rs, mcp/tests.rs, search/ai/mappings.rs, integration.test.ts, debug/+page.svelte.
2026-04-14	4514a832	Split files that grew too big	Split four large files: mcp/executor.rs (1718), commands/file_system.rs (1196), api-server/src/index.ts (1099), write_operations/integration_test.rs (1564).
2026-04-14	7514cb4e	Cut back the file-length allowlist	Initial baseline: seeded 31 entries when the file-length check was first added.
2026-04-08	e5820bb1	Bump GitHub Actions	actions/checkout v4→v6, jdx/mise-action v2→v4 (Node 20→24); cleared deprecation warnings.
2026-03-31	995f8c8e	Tighten linter rules	Replaced prettier with oxfmt across the entire repo (10–20× faster).
2026-03-31	6a45aa48	Tighten linter rules	Enabled oxfmt for the whole repo and reformatted Markdown, YAML, JSON, and analytics dashboard.
2026-03-31	1b54e1661	Tighten linter rules	Convention change: tab-width 4→2 across all frontend code and Markdown.
2026-03-30	59759230	Bump npm packages	Minor package bump to clear a warning.
2026-03-27	10ea3ed3	Refresh AGENTS.md	Split tooling docs: moved generic service-access docs out of repo; trimmed cloudflare.md, hetzner-vps.md, posthog.md, umami.md to Cmdr-specific content only.
2026-03-26	17ff5e2b	Purge old specs	Deleted all 46 spec files from docs/specs/; migrated architectural intent into ~14 colocated CLAUDE.md files.
2026-03-26	39086418	Split files that grew too big	Split indexing/mod.rs (3096→1850 lines): extracted enrichment.rs, event_loop.rs, events.rs.
2026-03-26	2d5b2989	Refresh AGENTS.md	Extracted MCP docs from AGENTS.md into a dedicated file and extended them.
2026-03-24	929556f2	Bump Rust crates	Bulk upgrade of 9 Rust deps (reqwest, rusqlite, tauri-plugin-mcp-bridge, notify-debouncer-full, mdns-sd, file_icon_provider, icns, image, nusb); also fixed RUSTSEC-2026-0067/0068.
2026-03-24	c17c210c	Split files that grew too big	Split indexing/search.rs (2361 lines) into search/{types,index,engine,query}.rs; split SearchDialog.svelte (1552) into orchestrator + sub-components; moved AI pipeline to search/ai/.
2026-03-24	52afe37a	AI-smell / inelegance review	Extracted 7 duplicated patterns across Rust and frontend (format loops, date helpers, lazy caches, reset blocks, log dedup, API envelope unwrap, issue-printing blocks).
2026-03-23	608af363	Bump npm packages	Bulk update of all main-app npm packages to latest.
2026-03-23	a61fa38b	Dead code sweep	Removed stale ESLint disable comments and dead helper functions.
2026-03-23	33ec2f27	AI-smell / inelegance review	Extracted start_write_operation, visible_entries, IoResultExt, FileEntry::new(), with_savepoint, and event-handler factories from the search dialog.
2026-03-18	36b3408c	Tighten linter rules	Stylelint enforcement: banned !important, raw colors, restricted font-weight/opacity; migrated ~26 rgba() calls to design tokens.
2026-03-18	e3259b0a	Tighten linter rules	Stylelint enforcement: banned raw px in padding/margin/gap; migrated ~180 values across 44 components.
2026-03-18	50f2b422	Tighten linter rules	Stylelint enforcement on font-size, border-radius, font-family, z-index design tokens.
2026-03-12	128c71ea	Security advisory sweep	tar 0.4.44→0.4.45 to clear two RUSTSEC advisories.
2026-03-11	d297a1a8	Refresh AGENTS.md	Slimmed AGENTS.md from 245 to 93 lines; extracted wrap-up checklist and planning workflow into Claude commands.
2026-03-11	ccf5cc7f	Purge old specs	Deleted all 19 docs/adr/ files; migrated decisions to colocated CLAUDE.md files; moved 3 evidence-rich ADRs to docs/notes/.
2026-03-11	4bead2b9	Tighten linter rules	Added a circular-dependency check for TS code.
2026-03-11	7ed1cea1	Dead code sweep	Removed all circular deps in TS code (post-check-enforcement cleanup).
2026-03-10	e16bd918	Split files that grew too big	Extracted scroll and search logic from the file viewer.
2026-03-10	8522e71f	Split files that grew too big	Extracted macOS/Linux-specific menu code into separate modules.
2026-03-06	2f7bff1a	Refresh CLAUDE.md files	Split monolithic infrastructure.md into 6 per-service files; merged checker and E2E docs into colocated CLAUDE.md files.
2026-03-05	0cd62e57	Bump npm packages	"Upgrade NPM packages to get rid of security vulns." Cleared known vulns.
2026-03-05	00880a0c	Tighten linter rules	Added Renovate for automated dep updates; removed govulncheck (covered by Renovate).
2026-03-03	347ae9bd	Refresh CLAUDE.md files	"Enrich CLAUDE.md files with intent": upgraded 25 CLAUDE.md files from structural inventories to intent-capturing docs.
2026-03-03	7ae7cd17	Refresh CLAUDE.md files	13 CLAUDE.md files fixed; 30 verified up to date.
2026-03-03	1d2fd4f6	Dead code sweep	Removed two unused type guards (isKeychainError, isMountError).
2026-02-26	337f6207	Split files that grew too big	Split the two biggest frontend files; tab-operations.ts (363) and rename-flow.svelte.ts (274) extracted; DualPaneExplorer 2514→2284, FilePane 1887→1667.
2026-02-26	cfae0db4	Split files that grew too big	Extracted dialog state from DualPaneExplorer (10 dialog state vars moved; 2289→2124 lines).
2026-02-25	ba86d874	Split files that grew too big	Split tauri-commands/file-viewer.ts into viewer-only + file-actions.ts + icons.ts + app-state.ts.
2026-02-25	35a42394	AI-smell / inelegance review	Deduplicated Settings CSS; removed duplicated class patterns across 11 sections.
2026-02-23	ab87bc5d	Refresh CLAUDE.md files	Added missing CLAUDE.md files and updated existing ones.
2026-02-22	79703a91	Bump Rust crates	Bumped Tauri npm packages to align with Tauri crate 2.10.x.
2026-02-21	d280cba0	Bump npm packages	Audit-driven: Svelte 5.49→5.53, Hono 4.11→4.12, fast-xml-parser 5.3.3→5.3.7.
2026-02-21	c26c7169	Bump Rust crates	nusb 0.1→0.2.2.
2026-02-21	974e2d3d	Bump Rust crates	Switched mtp-rs from a git dep to crates.io 0.1.0.
2026-02-14	2c805eff	Dead code sweep	Cleaned knip ignores and removed a chunk of dead code that surfaced.
2026-02-14	eac9e618	Purge old specs	Large doc overhaul: deleted old specs and notes (retained in git history); created CLAUDE.md files throughout the repo and added the architecture.md map.
2026-02-14	a7758f9b	Dead code sweep	Removed a few ESLint disables that were no longer needed.
2026-02-11	a05a2a24	Dead code sweep	Removed 5 unused objc2-app-kit features and 2 dummy imports.
2026-02-08	c0d8cc31	Pin GitHub Actions to commit SHAs	Pinned all GitHub Actions to commit SHAs (supply-chain safety sweep).
2026-02-08	50f705d1	Bump pinned tool versions in .mise.toml	Bumped Go to 1.25.7 in CI for crypto/tls vulnerability GO-2026-4337.
2026-02-08	8ee2dca7	Purge old specs	Deleted completed specs from docs/specs/.
2026-02-08	fe0bebca	AI-smell / inelegance review	Stripped tautological doc comments from Rust structs (20 files cleaned).
2026-02-08	a2073698	Dead code sweep	Removed dead code across the license server, Go scripts, frontend, and MTP.
2026-02-08	0d1c48ee	AI-smell / inelegance review	Replaced latinisms in code comments: "e.g." → "for example/like/such as" across ~50 instances.
2026-02-08	1ee4dfab	Dead code sweep	Deleted mock-only infrastructure tests (saveColumnSortOrder, resortListing).
2026-02-07	0a543395	Dead code sweep	Deleted 37 inline-reimplementation tests in streaming-loading.test.ts and integration.test.ts.
2026-02-07	b3afd0b7	Dead code sweep	Deleted tautological Rust tests (matching enums against themselves).
2026-02-07	cdab4448	Dead code sweep	Deleted 8 type-shape tests + 1 signature test from streaming-loading.test.ts.
2026-02-07	b45d4fd0	Dead code sweep	Rewrote licensing tests to test actual logic; replaced 329 lines of mock-the-mock tests.
2026-02-07	f3c60425	AI-smell / inelegance review	Removed verbose name-restating doc comments; ~50 JSDoc/Rust doc comments stripped across 4 files.
2026-02-06	2da8e6dd	Split files that grew too big	Split volume_copy.rs.
2026-02-06	c0bd500b	Split files that grew too big	Split listing/operations.rs.
2026-02-06	707a96a9	Split files that grew too big	Split connection.rs.
2026-02-06	e14c2893	Split files that grew too big	Split FilePane.svelte.
2026-02-06	04dc3deb	Split files that grew too big	Split DualPaneExplorer.svelte.
2026-02-06	c2f9df04	Review file structure	Reorganized frontend file layout.
2026-02-05	88428a93	Split files that grew too big	Split several big Rust files in one pass.
2026-02-05	1a37b352	Review file structure	Reorganized file_system module structure.
2026-02-03	e7f41d74	Dead code sweep	MTP: deleted dead code.
2026-01-28	7b188f29	Replace AI-generated content with handcrafted	Cleaned up AI-generated code that had slipped into the codebase.
2026-01-22	ababb825	Split files that grew too big	Split write_operations.rs because it grew too large to fit into agent contexts; first documented split driven by agent-context size.
2026-01-20	d327cf49	Tighten linter rules	#![warn(clippy::allow_attributes_without_reason)] + backfilled reasoning on every existing #[allow].
2026-01-20	1bc16349	AI-smell / inelegance review	Extracted all rgba() calls and var() fallback values from Svelte files into CSS variables; added a lint rule forbidding future fallbacks.
2026-01-18	cb5b8227	Refresh AGENTS.md	Improved AGENTS.md with better agent process descriptions.
2026-01-18	c8e365a3	AI-smell / inelegance review	Deduplicated BriefList and FullList Svelte components.
2026-01-18	165a19d7	AI-smell / inelegance review	Deduplicated Rust code across modules.
2026-01-18	7cbf6357	Dead code sweep	Removed unused CSS classes.
2026-01-18	ae142521	Security advisory sweep	Patched a vulnerable npm dep in the license-server app (earliest security-driven dep bump on record).
2026-01-13	df3c162a	Replace AI-generated content with handcrafted	Replaced AI-generated privacy policy with a handcrafted one.
2026-01-13	34bc38de	Purge old specs	Moved old specs into a new /specs folder (first spec reorganization event).
2026-01-13	eb47a916	Review file structure	Restructured the /docs folder.
2026-01-13	7d0d78fe	Refresh AGENTS.md	Cleaned up AGENTS.md.
2026-01-10	62421f48	Tighten linter rules	Made Clippy more thorough; added new deny-level lints.
2026-01-08	c0e764a7	Review file structure	Moved the app from repo root into apps/desktop/ to support a monorepo alongside the website; one-time structural reorganization.
2026-01-05	b338bf4b	Bump pinned tool versions in .mise.toml	Updated code to be compatible with Rust 1.92.0 (toolchain pin changed).
2026-01-04	a778dccd	Tighten linter rules	First introduction of Stylelint; fixed all existing CSS violations in one pass.
2025-12-30	6e4e630c	Bump Rust crates	Blanket update of all Rust deps.
2025-12-29	fa754490	Replace AI-generated content with handcrafted	Replaced AI-generated Dropbox icons with handcrafted ones.
2025-12-25	08740386	Refresh AGENTS.md	Added the very first AGENTS.md (root).
2025-12-25	9da9de16	Bump npm packages	First-ever npm bump: brought all frontend packages to latest right after project init.
```
