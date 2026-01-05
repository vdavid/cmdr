# Contributing

Thanks for your interest in contributing to Rusty Commander! The easiest way to contribute is to fork the repo, make
your changes, and submit a PR. This doc will help you get started.

## Dev setup

The project uses [mise](https://mise.jdx.dev) for tool version management. It handles Node, pnpm, and Go versions. Rust
is managed separately by `rustup`. This version is tested with Rust 1.92.0.

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

For an universal installer:

- `rustup target add x86_64-apple-darwin` once
- Then `pnpm tauri build --target universal-apple-darwin` each time.
- Then the binary is at `src-tauri/target/universal-apple-darwin/release/bundle/dmg/Rusty Commander_0.1.0_universal.dmg`

## Agent integration (MCP)

The app uses [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri) to let AI assistants (Claude Code,
Cursor, etc.) control this app: take screenshots, click buttons, read front-end logs, etc. It's quite helpful.

### Setting up your AI assistant

For `claude-code`, `cursor`, `vscode`, or `windsurf`, there is autoconfig available. Run this command in your terminal
for your specific client: `npx -y install-mcp @hypothesi/tauri-mcp-server --client <your-client>`.
([source](https://github.com/hypothesi/mcp-server-tauri)).

If the automated setup doesn't work for you, check the MCP documentation for your specific client. For example:

- [Claude Desktop](https://docs.anthropic.com/en/docs/agents-and-tools/mcp)
- [Cursor](https://docs.cursor.com/context/model-context-protocol)
- [Antigravity](https://medium.com/google-developer-experts/google-antigravity-custom-mcp-server-integration-to-improve-vibe-coding-f92ddbc1c22d)

This snippet will likely come handy:

```json
{
    "mcpServers": {
        "tauri": {
            "command": "npx",
            "args": ["-y", "@hypothesi/tauri-mcp-server"]
        }
    }
}
```

Since the agent shares the context with your IDE/client, enabling the MCP server makes the tools available to the agent
automatically.

Happy coding!
