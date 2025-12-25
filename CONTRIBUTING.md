# Contributing

Thanks for your interest in contributing to Rusty Commander! The easiest way to contribute is to fork the repo, make
your changes, and submit a PR. This doc will help you get started.

## Dev setup

The project uses [mise](https://mise.jdx.dev) for tool version management. It handles Node, pnpm, and Go versions. Rust
is managed separately by `rustup`.

1. Install mise: `brew install mise` (see [alternatives](https://mise.jdx.dev/getting-started.html))
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
3. Run `mise install` to set up Node, pnpm, and Go
4. Run `pnpm install` to install frontend dependencies

## Running the app

```bash
pnpm tauri dev
```

This starts both the Svelte frontend and the Rust backend with hot reload.

## Tooling

Run all checks before committing with `./scripts/check.sh`. And here is a more complete list:

```bash
./scripts/check.sh                # to run all checks before committing - USE THIS BY DEFAULT
./scripts/check.sh --rust         # to run Rust checks
./scripts/check.sh --svelte       # to run Svelte checks
./scripts/check.sh --check clippy # to run specific checks
./scripts/check.sh --help`        # for more options.
# Alternatively, some specific checks, but these are rarely needed:
cargo fmt                         # to format Rust code
cargo clippy                      # to lint Rust code
cargo audit                       # to check Rust dependencies for security vulnerabilities
cargo test                        # to run Rust tests
pnpm format                       # to format frontend code
pnpm lint --fix                   # to lint frontend code
pnpm test                         # to run frontend tests
```

## Building

```bash
pnpm tauri build
```

This creates a production build for your current platform in `src-tauri/target/release/`.

Happy coding!
