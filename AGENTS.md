# AGENTS.md

Welcome! This is Rusty Commander, blazing a fast, keyboard-driven, two-pane file manager built with Rust. First, see
[README.md](README.md) to get oriented.

Uses Rust, Tauri 2, Svelte 5, TypeScript, Tailwind 4. Targets macOS now, Win and Linux later.

- Dev server: `pnpm tauri dev` (launches Svelte + Rust with hot reload)
- Prod build: `pnpm tauri build`

## Architecture

- `/src-tauri/` - Rust/Tauri backend (lib + binary)
    - `Cargo.toml` - Dependencies: tauri v2, serde, notify (file watching), tokio
    - `deny.toml` - License and dependency policies (advisories disabled due to Tauri's transitive deps)
    - `clippy.toml` - Cognitive complexity threshold: 15
    - `rustfmt.toml` - Max width: 120, 4 spaces
- `/src/` - Svelte frontend
    - Uses SvelteKit with static adapter
    - TypeScript strict mode enabled
    - Tailwind CSS v4 for styling
- `/scripts/check/` - Go-based unified check runner (replaces individual scripts)
- `/e2e/` - Playwright end-to-end tests
- `/docs/` - Docs including `style-guide.md`

### Guidelines

- **Frontend components**: We keep them in `src/lib/` (SvelteKit convention)
- **Routes**: In `src/routes/` (SvelteKit file-based routing)
- **Rust modules**: We keep them in `src-tauri/src/`
- **Static assets**: In `/static/`

## Common tasks

- Updating dependencies: see [here](docs/workflows/update-dependencies.md)
- Adding a new Rust dependency: see [here](docs/workflows/add-rust-dependency.md)
- Adding a new npm dependency: see [here](docs/workflows/add-npm-dependency.md)
- Making key tech decisions: see [here](docs/workflows/documenting-key-tech-decisions.md) - Examples for when to use
  this: choosing between competing libraries/frameworks; changing build/test processes; adopting conventions that differ
  from defaults.
- Generating test data: see [here](docs/workflows/generating-test-files.md) - Creates folders with 1k-50k files for
  stress-testing.
- Running a specific Rust test: `cd src-tauri && cargo nextest run <test_name>`.
- Running a specific Svelte test: `pnpm vitest run -t "<test_name>"`
- Running a specific E2E test: `pnpm test:e2e --grep "<test_name>"` or `pnpm test:e2e <test-file>`
- Debugging front end: Open dev console in running app (`Cmd+Option+I` on macOS). Use temp `console.log`s as needed.
- Debugging Rust: Use `println!` or `dbg!` macros
- Regenerating the icon: `cargo tauri icon ./path/to/your-high-res-icon.png` (in project root)
- Adding a new library: NEVER rely on your training data! ALWAYS use npm/ncu, or another source to find the latest
  versions of libraries. Check out their GitHub, too, and see if they are active. Check Google/Reddit for the latest
  best solutions!

## Code style

ALWAYS read the [full style guide](docs/style-guide.md) before touching the repo!

## Checks

ALWAYS run `./scripts/check.sh` before committing. This is the single source of truth for all checks. CI runs it too.

The check script is written in Go, runs all linters (with auto fixing), formatters, and tests.

Can use also `./scripts/check.sh --rust`, `./scripts/check.sh --svelte`, `./scripts/check.sh --check clippy` or similar,
or `./scripts/check.sh --help` to see all options.

Can also use `cargo fmt`, `cargo clippy`, `cargo audit`, `cargo deny check`, `cargo nextest run`, `pnpm format`,
`pnpm lint --fix`, `pnpm test`, `pnpm test:e2e` as needed.

GitHub Actions workflow in `.github/workflows/ci.yml`:

## Things to avoid

- ❌ Don't commit without running `./scripts/check.sh`
- ❌ Don't use classes in TypeScript (use functional components/modules)
- ❌ Don't add JSDoc that just repeats types or obvious function names
- ❌ Don't use `any` type (ESLint will error)
- ❌ Don't ignore linter warnings (fix them or justify with a comment)
- ❌ Don't add dependencies without checking licenses (`cargo deny check`)

## Decisions

See [docs/adr](docs/adr) for all key technical decisions, and the
[How to document important technical decisions](docs/workflows/documenting-key-tech-decisions.md) process.

- **Check script is in Go** (not Bash) for better cross-platform support and maintainability. See
  [ADR-001](docs/adr/001-use-go-for-check-script.md)
- **cargo-nextest** is used instead of `cargo test` for speed and better output. See
  [ADR-002](docs/adr/002-use-cargo-nextest.md)
- **deny.toml advisories check is off** because Tauri depends on unmaintained crates we can't control. See
  [ADR-003](docs/adr/003-disable-cargo-deny-advisories.md)
- **Prettier, ESLint, rustfmt, clippy** all auto-fix locally but only check in CI (enforced by `--ci` flag). See
  [ADR-004](docs/adr/004-auto-fix-locally-check-in-ci.md)
- **Scoped CSS for file explorer, Tailwind for other UI**: File list components use scoped CSS for performance with
  large lists (50k+ files), while other UI (settings, modals) uses Tailwind. See
  [ADR-005](docs/adr/005-scoped-css-for-file-explorer.md)
- **Clippy `--allow-dirty --allow-staged`** is used locally to allow auto-fixes even with uncommitted changes

## Useful references

- Tauri docs: https://tauri.app/v2/
- Svelte 5 docs: https://svelte.dev/docs/svelte/overview
- SvelteKit docs: https://svelte.dev/docs/kit/introduction
- Cargo-deny docs: https://embarkstudios.github.io/cargo-deny/
- Style guide: `docs/style-guide.md`
- Contributing guide: `CONTRIBUTING.md`

## Questions?

If something is unclear, check:

1. The style guide (`docs/style-guide.md`)
2. The contributing guide (`CONTRIBUTING.md`)
3. The check script usage (`./scripts/check.sh --help`)
4. The CI workflow (`.github/workflows/ci.yml`)

Happy coding! 🦀✨
