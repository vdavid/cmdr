# Automated updates

This document describes how Cmdr checks for and installs updates automatically.

## Overview

Cmdr uses Tauri's built-in updater plugin to deliver updates:

1. App checks for updates on startup and every 60 minutes
2. If an update is available, it downloads silently in the background
3. User sees a "Restart to update" notification when ready
4. Clicking restart applies the update and relaunches the app

Updates are signed with Ed25519 to ensure authenticity. The app won't install anything that doesn't match the embedded public key.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          Update flow                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐     ┌─────────────────┐     ┌───────────────┐  │
│  │  GitHub Actions │────▶│  GitHub         │────▶│ getcmdr.com   │  │
│  │  (build+sign)   │     │  Releases       │     │ /latest.json  │  │
│  └─────────────────┘     └─────────────────┘     └───────┬───────┘  │
│                                                          │          │
│                                                          ▼          │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Cmdr app                                                     │  │
│  │  1. Fetches latest.json                                       │  │
│  │  2. Compares versions                                         │  │
│  │  3. Downloads .tar.gz from GitHub Releases                    │  │
│  │  4. Verifies Ed25519 signature                                │  │
│  │  5. Shows "Restart to update" notification                    │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Update manifest

The app fetches `https://getcmdr.com/latest.json` to check for updates:

```json
{
    "version": "0.3.1",
    "notes": "### Added\n- New feature...",
    "pub_date": "2026-01-14T00:54:48Z",
    "platforms": {
        "darwin-universal": {
            "signature": "base64-encoded-ed25519-signature",
            "url": "https://github.com/vdavid/cmdr/releases/download/v0.3.1/Cmdr_universal.app.tar.gz"
        },
        "darwin-aarch64": { ... },
        "darwin-x86_64": { ... }
    }
}
```

All three macOS platforms point to the same universal binary. This ensures both Apple Silicon and Intel Macs find a matching update.

## Implementation

### Frontend (`apps/desktop/src/lib/updater.svelte.ts`)

The updater service manages the update lifecycle:

| Export                 | Description                                                           |
|------------------------|-----------------------------------------------------------------------|
| `startUpdateChecker()` | Starts checking on launch and every 60 min. Returns cleanup function. |
| `checkForUpdates()`    | Manually triggers an update check                                     |
| `getUpdateState()`     | Returns current state: `idle`, `checking`, `downloading`, or `ready`  |
| `restartToUpdate()`    | Relaunches the app to apply the downloaded update                     |

### UI (`apps/desktop/src/lib/UpdateNotification.svelte`)

A toast notification that appears in the bottom-right corner when an update is ready. Shows "Restart to update" with a button to trigger the restart.

### Configuration

**Production** (`tauri.conf.json`):
```json
"plugins": {
    "updater": {
        "endpoints": ["https://getcmdr.com/latest.json"],
        "pubkey": "base64-encoded-public-key"
    }
}
```

**Development** (`tauri.dev.json`):
```json
"plugins": {
    "updater": {
        "endpoints": ["http://localhost:4321/latest.json"]
    }
}
```

### Capabilities

The updater requires these permissions in `capabilities/default.json`:
- `updater:default` — allows checking and downloading updates
- `process:allow-restart` — allows relaunching the app

## Release workflow

When you push a version tag (for example, `v0.3.2`), the GitHub Actions release workflow:

1. Builds a universal macOS binary (aarch64 + x86_64)
2. Signs the `.app.tar.gz` with Ed25519 using `TAURI_SIGNING_PRIVATE_KEY`
3. Uploads artifacts to GitHub Releases
4. Updates `apps/website/public/latest.json` with the new version and signature
5. Triggers a website deploy so the manifest is live

See [Releasing guide](../guides/releasing.md) for step-by-step instructions.

## Logging

The updater logs to the backend via `feLog()`. Example output:

```
[updater] Started (endpoint: getcmdr.com)
[updater] Checking for updates (current: v0.3.0)...
[updater] Update available: v0.3.0 → v0.3.1
[updater] v0.3.1 downloaded, restart to apply
```

On error:
```
[updater] Check failed: Download request failed with status: 404 Not Found
```

## Local testing

To test updates locally without deploying:

1. Start the website dev server (serves `latest.json`):
   ```bash
   cd apps/website && pnpm dev
   ```

2. Edit `apps/website/public/latest.json` — set a version higher than your local build

3. Run the app in dev mode:
   ```bash
   cd apps/desktop && pnpm tauri dev
   ```

4. The app checks `localhost:4321/latest.json` and shows the update notification

Note: The actual download will fail locally since there's no signed artifact. This flow is useful for testing the detection and UI.

## Security

- **Signature verification**: Every update is verified against the embedded Ed25519 public key before installation
- **HTTPS**: Production endpoint uses HTTPS
- **No downgrade**: Tauri won't install older versions by default
- **Signed releases**: Only CI can sign releases (private key is a GitHub secret)
