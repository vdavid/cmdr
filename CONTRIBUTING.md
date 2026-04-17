# Contributing

Thanks for your interest in contributing to Cmdr! The easiest way to contribute is to fork the repo, make your changes,
and submit a PR. This doc will help you get started.

Note: This doc is entirely for humans. AI agents always read [AGENTS.md](AGENTS.md) and the colocated `CLAUDE.md` files
instead of this file.

## Dev setup

The project uses [mise](https://mise.jdx.dev) for tool version management. It handles Node, pnpm, and Go versions. Rust
is managed separately by `rustup`. This version is tested with Rust 1.92.0.

1. Install mise: `brew install mise` (see [alternatives](https://mise.jdx.dev/getting-started.html))
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
3. Run `mise install` to set up Node, pnpm, and Go
4. Run `cd apps/desktop && pnpm install` to install frontend dependencies

## Dev signing certificate (macOS, optional)

Cmdr stores SMB credentials in macOS Keychain. Keychain ties item access to the binary's code signature — production
builds are signed, so users get a one-time prompt. But dev/E2E builds are ad-hoc signed (changes every rebuild), which
triggers a Keychain password prompt on every restart.

To avoid this, create a local "Cmdr Dev" code signing certificate:

1. Open **Keychain Access.app**
2. Menu: **Keychain Access → Certificate Assistant → Create a Certificate...**
3. Name: `Cmdr Dev`, Identity Type: **Self Signed Root**, Certificate Type: **Code Signing**
4. Click **Create**, then trust it for code signing:
   ```bash
   security find-certificate -c "Cmdr Dev" -p > /tmp/cmdr-dev.pem
   security add-trusted-cert -p codeSign -r trustRoot -k ~/Library/Keychains/login.keychain-db /tmp/cmdr-dev.pem
   rm /tmp/cmdr-dev.pem
   ```
5. Verify: `security find-identity -v -p codesigning` should list "Cmdr Dev".

Once the certificate exists, the E2E check script (`./scripts/check.sh --check desktop-e2e-playwright`) auto-signs the
binary before launching. For manual E2E runs, sign after building:

```bash
codesign --force -s "Cmdr Dev" ./target/$(rustc -vV | grep host | cut -d' ' -f2)/release/Cmdr
```

If you already have Cmdr Keychain items from unsigned builds, delete them so they get re-created with the correct ACL:

```bash
security delete-generic-password -s "Cmdr" -a "smb://yourserver/yourshare"
```

## Running the app

```bash
pnpm dev
```

This starts both the Svelte frontend and the Rust backend with hot reload.

To test with a virtual MTP device (simulated Android phone):

```bash
cd apps/desktop && pnpm tauri dev -c src-tauri/tauri.dev.json --features virtual-mtp
```

## Debug window

In dev mode, press **Cmd+D** to open a debug window. This window is only available in dev builds and provides:

- **Dark mode toggle**: Switch between light and dark themes without changing your system settings
- **Navigation history**: Real-time view of back/forward history for both panes, showing current position and available
  history entries

The debug window is a separate, movable window that updates in real-time as you navigate.

## Logging

### Rust

Rust uses the standard `RUST_LOG` env var for log level control. The `pnpm dev` command sets a sensible default
(`smb=warn,sspi=warn,info`) to silence noisy external crate logs. Override with your own `RUST_LOG` as needed:

```bash
# Only warnings and errors
RUST_LOG=warn pnpm dev

# Verbose network debugging
RUST_LOG=cmdr_lib::network=debug,info pnpm dev

# Unsilence MCP logs by running the command directly, without the env vars built-in to `pnpm dev`
pnpm --filter @cmdr/desktop tauri dev
```

Module paths follow the Rust crate structure: `cmdr_lib::mcp`, `cmdr_lib::network`, `cmdr_lib::licensing`, etc.

## Workflow (Claude Code commands)

These are slash commands for Claude Code (type `/command-name` in the CLI):

- `/plan` — use when starting a feature
- `/wrap-up` — use before finishing work
- `/release` — prepare a release (changelog, versioning, roadmap)
- `/commit-draft` — draft a commit message for staged changes

## Tooling

Run all checks before committing with `./scripts/check.sh`. And here is a more complete list:

```bash
./scripts/check.sh                # to run all checks before committing - USE THIS BY DEFAULT
./scripts/check.sh --rust         # to run Rust checks
./scripts/check.sh --svelte       # to run Svelte checks
./scripts/check.sh --check clippy # to run specific checks
./scripts/check.sh --help`        # for more options.
# Alternatively, some specific checks (run from apps/desktop/), but these are rarely needed:
cd apps/desktop/src-tauri
cargo fmt                         # to format Rust code
cargo clippy                      # to lint Rust code
cargo audit                       # to check Rust dependencies for security vulnerabilities
cargo test                        # to run Rust tests
cd apps/desktop
pnpm format                       # to format frontend code
pnpm lint --fix                   # to lint frontend code
pnpm test                         # to run frontend tests
```

## Linux testing (Ubuntu VM)

The Linux E2E tests run against the real Tauri app with WebKitGTK. Since macOS doesn't have a WebDriver for WKWebView,
we need a Linux environment. We use a UTM virtual machine (Apple Virtualization) with Ubuntu, connected to the LAN at
`192.168.1.97`. The macOS repo is shared via VirtioFS so edits on either side are instant, but uses custom bind mounts
to avoid `node_modules` and build folders overwriting each other between the host mac and the VM.

How to use it for testing the app:

1. Start the VM
2. `cd ~/cmdr`
3. `pnpm install` if it's been a while or you've added new deps
4. `mountpoint /mnt/cmdr/cmdr/target && mountpoint /mnt/cmdr/cmdr/node_modules` to verify the bind mounts are healthy
   - If either mountpoint check fails, run `sudo mount -a` and re-check.
5. `eval "$(mise activate bash)"` to activate mise. It sets up Node/pnpm/Go — not available in the default SSH shell.

From here, either **run the app** or **run E2E tests**:

```bash
# a) Run the app (dev mode with hot reload)
cd ~/cmdr
WEBKIT_DISABLE_COMPOSITING_MODE=1 pnpm dev

# b) Run E2E tests (in Docker — same path CI runs)
cd ~/cmdr/apps/desktop
pnpm test:e2e:linux
```

See `apps/desktop/test/e2e-linux/CLAUDE.md` for VNC debugging, VM setup details, and WebKitGTK quirks.

## Building

From repo root:

```bash
pnpm build
```

Or from the desktop app directory:

```bash
cd apps/desktop
pnpm tauri build
```

This creates a production build for your current platform in `apps/desktop/src-tauri/target/release/`.

For an universal installer:

- `rustup target add x86_64-apple-darwin` once
- Then `cd apps/desktop && pnpm tauri build --target universal-apple-darwin` each time.
- Then the binary is at
  `apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/Cmdr_0.1.0_universal.dmg`

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

Or add it via CLI like:

Since the agent shares the context with your IDE/client, enabling the MCP server makes the tools available to the agent
automatically.

## Cloudflare access (API server)

The API server is a Cloudflare Worker. To deploy it or run `wrangler` commands, you need a Cloudflare API token.

1. Go to https://dash.cloudflare.com/profile/api-tokens → **Create Token** → **Custom token**
2. Permissions:
   - `Account / Workers Scripts / Edit`
   - `Account / Account Analytics / Read`
   - `Account / Workers Scripts / Edit`
   - `Zone / Workers Routes / Edit`
   - `Zone / DNS / Edit`
3. Account resources: the Cmdr account only
4. Add to macOS Keychain:
   ```sh
   security add-generic-password -a "$USER" -s "CLOUDFLARE_API_TOKEN" -w "your-token"
   ```

Wrangler picks up `CLOUDFLARE_API_TOKEN` from the environment — the shell profile exports it from Keychain on startup.

## PostHog access (website analytics)

PostHog is used for session replay and heatmaps on getcmdr.com. To use the PostHog management API (for example, to
update project settings), you need a personal API key.

1. Go to https://eu.posthog.com/settings/user-api-keys → **Create personal API key**
2. Scope it to the Cmdr project
3. Add to macOS Keychain:
   ```sh
   security add-generic-password -a "$USER" -s "POSTHOG_API_KEY" -w "phx_your-key"
   ```

See [posthog.md](docs/tooling/posthog.md) for API recipes.

## Paddle access (payments)

Paddle handles payments and subscriptions. Two API keys are needed — one for live, one for sandbox (testing).

1. Go to https://vendors.paddle.com → **Developer tools** → **Authentication** → **Generate API key**
2. Repeat for sandbox at https://sandbox-vendors.paddle.com
3. Add both to macOS Keychain:
   ```sh
   security add-generic-password -a "$USER" -s "PADDLE_LIVE_API_KEY" -w "your-live-key"
   security add-generic-password -a "$USER" -s "PADDLE_SANDBOX_API_KEY" -w "your-sandbox-key"
   ```

See the Paddle generic tooling doc for API recipes.

## Cloudflare Access (analytics dashboard)

The analytics dashboard at `analdash.getcmdr.com` is behind Cloudflare Access. To fetch reports via the API, you need a
service token.

1. Go to https://one.dash.cloudflare.com → **Access** → **Service Auth** → **Create Service Token**
2. Add both values to macOS Keychain:
   ```sh
   security add-generic-password -a "$USER" -s "CF_ACCESS_CLIENT_ID_EXPIRES_2027_03_22" -w "your-client-id"
   security add-generic-password -a "$USER" -s "CF_ACCESS_CLIENT_SECRET_EXPIRES_2027_03_22" -w "your-client-secret"
   ```

The token expires 2027-03-22. See [analytics-dashboard.md](docs/tooling/analytics-dashboard.md) for usage.

## ngrok access (tunnels)

ngrok exposes local servers to the internet — useful for testing webhooks (for example, Paddle) against your local API
server.

1. Go to https://dashboard.ngrok.com → **Your Authtoken** (or **API** → **API Keys** for the API key)
2. Add to macOS Keychain:
   ```sh
   security add-generic-password -a "$USER" -s "NGROK_API_KEY" -w "your-api-key"
   ```

See the ngrok generic tooling doc for API recipes.

## API server local dev

To run the API server locally (for testing license activation, generating test keys, etc.), you need a `.dev.vars` file
with Paddle and Resend secrets. See the [API server README](apps/api-server/README.md#local-development) for the full
setup. Ask a maintainer for the current values if you don't have dashboard access.

## Self-hosted GitHub Actions macOS runner (maintainers)

The release workflow runs on a self-hosted macOS runner and can save a bunch of GitHub Actions credits.

To set one up:

1. Go to [repo](https://github.com/vdavid/cmdr) → **Settings** → **Actions** → **Runners** → **New self-hosted runner**
2. Select **macOS** and **ARM64**
3. Follow GitHub's instructions to download, configure, and register the runner, and run it to test it works.
4. Quit it, then install it as a launchd service so it starts on boot:
   ```bash
   ./svc.sh install
   ./svc.sh start
   ```
5. Make sure the runner has all build dependencies: Rust (`rustup`), Node, pnpm, Go (all via `mise install`), and Xcode
   CLI tools. You need these to build the app anyway.
6. Prevent sleep in **System Settings → Energy** so the runner stays available during releases.

The runner auto-receives the labels `self-hosted`, `macOS`, `ARM64`, which the release workflow matches on. Apple
Silicon can cross-compile x86_64 and universal builds, so a single ARM64 runner handles all three architectures. Yay!

## Infrastructure access (maintainers)

If you have SSH access to the production server (`ssh hetzner`) and credentials for services like Umami, Cloudflare, and
Paddle, see [docs/architecture.md](docs/architecture.md#tooling-and-infrastructure) for the full map of per-service
docs.

Happy coding!
