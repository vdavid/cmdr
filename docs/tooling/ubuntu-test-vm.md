# Ubuntu test VM

A local Ubuntu VM for testing Cmdr on Linux. Used for manual testing, debugging WebKitGTK-specific
behavior, and future cross-platform development.

## Quick start

```bash
# From macOS — SSH into the VM
ssh veszelovszki@192.168.64.6

# Inside the VM — run Cmdr
eval "$(mise activate bash)"
cd ~/cmdr
pnpm dev
```

For GUI interaction (pressing keys, clicking), use the VM window in UTM directly — not SSH.

## VM specs

| Property | Value                                           |
|----------|-------------------------------------------------|
| Host app | UTM (Apple Virtualization engine)               |
| OS       | Ubuntu 25.10 (aarch64)                          |
| RAM      | 12 GB                                           |
| CPU      | 6 cores                                         |
| Disk     | 64 GB                                           |
| Username | `veszelovszki`                                  |
| SSH      | `ssh veszelovszki@192.168.64.6`                 |
| IP       | `192.168.64.6` (DHCP — may change after reboot) |

The VM IP is assigned by macOS DHCP. If SSH stops working, check the IP inside the VM:

```bash
ip addr show | grep 'inet ' | grep -v 127.0.0.1
```

## File layout

| Path                          | What                                       |
|-------------------------------|--------------------------------------------|
| `/mnt/cmdr/cmdr`              | VirtioFS mount of the macOS `cmdr` repo    |
| `~/cmdr`                      | Symlink to `/mnt/cmdr/cmdr`                |
| `~/cmdr-node-modules/root`    | VM-local `node_modules` for monorepo root  |
| `~/cmdr-node-modules/desktop` | VM-local `node_modules` for `apps/desktop` |

### Shared folder

The macOS `cmdr` directory is shared via UTM's VirtioFS. Edits on either side are instant — Vite
HMR picks up `.svelte`/`.ts` changes in ~1-3s. The mount is configured in `/etc/fstab`:

```
share /mnt/cmdr virtiofs defaults 0 0
```

### node_modules isolation

Linux and macOS need different native binaries in `node_modules`. To prevent them from overwriting
each other, the VM bind-mounts local directories over the shared `node_modules` paths:

```
~/cmdr-node-modules/root    → ~/cmdr/node_modules
~/cmdr-node-modules/desktop → ~/cmdr/apps/desktop/node_modules
```

These are in `/etc/fstab` and mount automatically on boot. If you ever need to rebuild them:

```bash
rm -rf ~/cmdr-node-modules/root/* ~/cmdr-node-modules/desktop/*
cd ~/cmdr && pnpm install
```

## Installed toolchain

Managed by [mise](https://mise.jdx.dev/) — versions come from `.mise.toml` in the repo root.

| Tool | Installed via | Notes                              |
|------|---------------|------------------------------------|
| Rust | rustup        | Version from `rust-toolchain.toml` |
| Node | mise          | Version from `.mise.toml`          |
| pnpm | mise          | Version from `.mise.toml`          |
| Go   | mise          | Version from `.mise.toml`          |

System packages (Tauri prerequisites, WebKitGTK dev libs, etc.) are installed via apt.

**Important**: Always activate mise before running commands:

```bash
eval "$(mise activate bash)"
```

This is in `~/.bashrc` so it runs automatically in interactive shells, but not in SSH one-liners.
For SSH one-liners, prefix with `eval "$(mise activate bash)" &&`.

## Common tasks

### Run Cmdr in dev mode (with hot reload)

```bash
cd ~/cmdr && pnpm dev
```

Frontend changes hot reload via Vite. Rust changes require restarting.

### Run the E2E Linux tests natively

```bash
cd ~/cmdr/apps/desktop && pnpm test:e2e:linux:native
```

### Run all checks

```bash
cd ~/cmdr && ./scripts/check.sh
```

### Rebuild node_modules after lockfile changes

```bash
cd ~/cmdr && pnpm install
```

### Enable debug logging

```bash
RUST_LOG=debug pnpm dev                               # Everything
RUST_LOG=smb=warn,sspi=warn,info pnpm dev              # Suppress noisy SMB logs
RUST_LOG=cmdr_lib::file_system=debug,info pnpm dev     # Specific module
```

## Why this VM exists

macOS uses WKWebView which has no WebDriver implementation, so WebDriver-based E2E tests only run
on Linux (WebKitGTK has WebKitWebDriver). This VM provides a real Linux desktop for:

- **Manual testing**: Press keys, click around, observe real GTK behavior
- **Debugging WebKitGTK quirks**: Keyboard events, focus, and event delegation behave differently
  from macOS WebKit
- **E2E test development**: Run tests natively without Docker overhead
- **Cross-platform prep**: Test Cmdr on Linux as a step toward full cross-platform support

## Troubleshooting

### Shared folder not mounted after reboot

```bash
sudo mount -a
ls ~/cmdr/CLAUDE.md  # Should exist
```

### node_modules bind mounts not active

```bash
mountpoint -q ~/cmdr/node_modules || sudo mount -a
```

### VM IP changed

Check inside the VM: `ip addr show | grep 'inet ' | grep -v 127.0.0.1`

### pnpm/node not found

```bash
eval "$(mise activate bash)"
```
