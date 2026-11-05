# Adding a workspace crate

The steps for a new crate under `crates/`, in order. Each points to the doc that owns the mechanism; read that, not a
copy here. A recent small model to copy: `crates/cmdr-sort/` (vocabulary only), or `crates/cmdr-archive/` for a backend.

1. **Join the workspace.** Add the path to the root `Cargo.toml`'s `[workspace] members`. Copy a sibling's `[package]`
   block (`version = "0.0.0"`, `edition = "2024"`, `license = "LicenseRef-BSL-1.1"`, `publish = false`) and its
   `[lints] workspace = true`. A tool or a platform-bound crate declares `kind` / `platforms` in
   `[package.metadata.cmdr]`; the default is a portable `app` member (`scripts/check/checks/DETAILS.md` § "Workspace
   geometry: which members a Rust check reaches").
2. **Get scanned and tested.** `pnpm check workspace-member-coverage` proves every lane and scanner reaches the member.
   A Rust check never hardcodes a crate path: scanners take theirs from `ScannerRoots` / `ScannerMemberKinds`
   (`scripts/check/checks/CLAUDE.md`, and § "Workspace member coverage" in its `DETAILS.md`).
3. **Stay app-free.** Add the crate to `guardedIndexCrates` in `scripts/check/checks/index-crate-isolation.go`. Unless
   it's shared vocabulary like `cmdr-fs`, also give it a `surfaceGuardedCrates` entry in
   `index-crate-isolation-ceilings.go` at exactly what it exposes, with the rationale in the entry and in the crate's
   `DETAILS.md`. Raising a ceiling later is the one allowlist carve-out an agent may take, and only when the wider
   surface is genuinely better, said in that `DETAILS.md` and the commit (`scripts/check/checks/DETAILS.md` § "Allowlist
   consent"). Then `pnpm check index-crate-isolation`.
4. **No module cycles.** Run `pnpm check module-cycles` on the new crate before landing: it's slow and not in `--fast`,
   so a cycle there has slipped through to CI and failed it (`scripts/check/checks/DETAILS.md` § "Rust module cycles").
5. **The browser demo.** If `cmdr-listing-wasm` links the crate (directly or through another path dep), add it to the
   demo image's three input lists, or getcmdr.com serves a stale engine (`crates/cmdr-listing-wasm/DETAILS.md` § The
   build, "Every crate the module links is a demo image input").
6. **Dependencies.** Each new dep follows `AGENTS.md` § Dependencies: latest stable version checked at the source, and
   `pnpm check desktop-rust-cargo-deny` green.
7. **Docs.** Give the crate a `CLAUDE.md` + `DETAILS.md` pair (`AGENTS.md` § Docs), add it to `AGENTS.md` § File
   structure's `crates/` list with a two-to-four-word gloss, and to `docs/architecture.md` § Workspace crates: the
   app-free and capped-surface lists in its intro, plus a bullet saying what it owns and where its docs live.
