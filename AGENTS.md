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

## Deps management

### Updating dependencies

- Frontend: `ncu` to see them, then `ncu -u && pnpm install` to apply.
- Rust (in `src-tauri/`): `cargo update && cargo outdated` (update within semver ranges; check for newer versions)
- Current version constraints: see `.mise.toml`. We try to use the latest.
- Rust: stable channel (see `rust-toolchain.toml`)

## Common tasks

### Adding a new Rust dependency

1. Add to `src-tauri/Cargo.toml`
2. Run `cargo build` to update `Cargo.lock`
3. Check licenses with `cargo deny check licenses`

### Adding a new npm dependency

1. Run `pnpm add <package>` (or `pnpm add -D <package>` for dev deps)
2. Commit both `package.json` and `pnpm-lock.yaml`

### Running a specific test

**Rust**:

```bash
cd src-tauri
cargo nextest run <test_name>
```

**Svelte**:

```bash
pnpm vitest run -t "<test_name>"
```

### Debugging

- Tauri dev tools: Open dev console in running app (Cmd+Option+I on macOS)
- Rust logs: Use `println!` or `dbg!` macros
- Frontend logs: Use `console.log` (but remember to remove before committing, ESLint warns)

## File structure tips

- **Frontend components**: Keep them in `src/lib/` (SvelteKit convention)
- **Routes**: In `src/routes/` (SvelteKit file-based routing)
- **Rust modules**: Keep them in `src-tauri/src/`
- **Static assets**: In `/static/`

## Things to avoid

- ❌ Don't commit without running `./scripts/check.sh`
- ❌ Don't use classes in TypeScript (use functional components/modules)
- ❌ Don't add JSDoc that just repeats types or obvious function names
- ❌ Don't use `any` type (ESLint will error)
- ❌ Don't ignore linter warnings (fix them or justify with a comment)
- ❌ Don't add dependencies without checking licenses (`cargo deny check`)

## Quirks and gotchas

- **cargo-nextest** is used instead of `cargo test` for speed and better output
- **deny.toml advisories check is off** because Tauri depends on unmaintained crates we can't control
- **Check script is in Go** (not Bash) for better cross-platform support and maintainability
- **Clippy `--allow-dirty --allow-staged`** is used locally to allow auto-fixes even with uncommitted changes
- **Prettier, ESLint, rustfmt, clippy** all auto-fix locally but only check in CI (enforced by `--ci` flag)

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
