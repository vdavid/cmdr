# Linux and Windows versions

This document analyzes the current macOS-only components in Cmdr and what's needed to support Linux and Windows.

## Current platform support overview

| Category              | Lines  | Platform          | Notes                                          |
|-----------------------|--------|-------------------|------------------------------------------------|
| **Core file system**  | ~4,500 | ✅ Cross-platform  | Listing, sorting, watching                     |
| **Write operations**  | ~2,500 | ✅ Cross-platform* | Copy/move/delete (with macOS optimizations)    |
| **Icons**             | ~650   | ⚠️ Mostly macOS   | `macos_icons.rs` + `file_icon_provider` crate  |
| **Volumes/locations** | ~660   | ❌ macOS only      | Finder sidebar, attached volumes, cloud drives |
| **Network/SMB**       | ~2,800 | ❌ macOS only      | Bonjour discovery, SMB client, Keychain        |
| **Permissions**       | ~60    | ❌ macOS only      | Full Disk Access check                         |
| **Sync status**       | ~150   | ❌ macOS only      | Dropbox/iCloud sync badges                     |
| **UI commands**       | ~200   | ⚠️ Partial        | Quick Look, Get Info, Show in Finder are macOS |
| **MCP server**        | ~1,100 | ✅ Cross-platform  | Works everywhere                               |
| **Menu system**       | ~600   | ✅ Cross-platform  | Tauri handles it                               |

## What already works on Linux

The app compiles on Linux (verified by CI). These features work:

1. **File browsing**: listing directories, sorting, filtering, searching
2. **Write operations**: copy, move, delete (falls back to standard fs ops)
3. **File watching**: notify crate is cross-platform
4. **Menu system**: Tauri's menu is cross-platform
5. **Settings/config**: stored via tauri-plugin-store
6. **MCP server**: for AI agent integration
7. **Licensing**: server validation works anywhere

## macOS-only modules

### 1. Volumes/locations sidebar (`src/volumes/`)

**~660 lines**

**What it does on macOS:**

- Lists Finder favorites (Desktop, Documents, Downloads, etc.)
- Discovers main volume ("Macintosh HD")
- Lists attached volumes from `/Volumes`
- Detects cloud drives (Dropbox, iCloud, Google Drive, OneDrive)
- Watches for volume mount/unmount events

**Linux equivalent needed:**

- Parse `/etc/mtab` or `/proc/mounts` for mounted filesystems
- XDG user directories (`~/.config/user-dirs.dirs`)
- Monitor `/media/$USER` and `/mnt` for removable media
- Use `udisks2` D-Bus API or GIO/GVfs for GNOME integration
- Watch for mount events via `inotify` on `/etc/mtab`

**Windows equivalent needed:**

- List drive letters via `GetLogicalDrives()`
- Use `SHGetKnownFolderPath()` for special folders
- Monitor volume changes via `RegisterDeviceNotification()`

### 2. Network/SMB (`src/network/`)

**~2,800 lines**

**What it does on macOS:**

- Bonjour (mDNS/DNS-SD) host discovery via `NSNetService`
- SMB share listing via `smb` crate (macOS-only)
- Keychain integration for credential storage
- Share mounting via `NetFS` framework
- Known shares persistence

**Linux equivalent needed:**

- Avahi for mDNS discovery (`avahi-browse` or libavahi)
- SMB: `libsmbclient` or shell out to `smbclient`
- Secret Service API via `libsecret` or `keyring` crate
- Mount via `mount.cifs` or `gio mount`

**Windows equivalent needed:**

- NetBIOS/WSD for network discovery
- Native SMB via WNet APIs (`WNetEnumResource`, `WNetAddConnection2`)
- Windows Credential Manager via `CredRead`/`CredWrite`
- Network drives via `WNetUseConnection`

**Note:** The `smb` crate used for share listing is macOS-only. A cross-platform solution would need a different library
or approach.

### 3. Icons (`src/macos_icons.rs`, `src/icons.rs`)

**~650 lines combined**

**What it does on macOS:**

- Uses `file_icon_provider` crate (macOS-only via `NSWorkspace`)
- Custom `.icns` file parsing for app icons
- Icon caching with base64 WebP encoding

**Linux equivalent needed:**

- XDG icon theme lookup via `freedesktop-icons` crate
- GTK icon theme integration
- MIME type to icon mapping

**Windows equivalent needed:**

- Windows Shell API (`SHGetFileInfo`, `IShellItemImageFactory`)
- Extract icons from `.exe` and `.dll` files

### 4. UI polish commands (`src/commands/ui.rs`)

**~150 lines of macOS-specific code**

| Feature        | macOS implementation  | Linux equivalent                            | Windows equivalent       |
|----------------|-----------------------|---------------------------------------------|--------------------------|
| Quick Look     | `qlmanage -p`         | `gnome-sushi`, `gloobus-preview`, or custom | Windows Preview Handlers |
| Show in Finder | `open -R`             | `xdg-open` parent + select (tricky)         | `explorer /select,`      |
| Get Info       | AppleScript to Finder | `nautilus --properties` or custom panel     | Shell `Properties` verb  |

### 5. Sync status (`src/file_system/sync_status.rs`)

**~150 lines**

**What it does on macOS:**

- Reads extended attributes for Dropbox (`com.dropbox.attributes`)
- Reads extended attributes for iCloud sync state
- Returns sync status badges (synced, syncing, error, etc.)

**Linux equivalent:**

- Dropbox: Check `~/.dropbox` status files or D-Bus interface
- Different per cloud provider—may need provider-specific handling

**Windows equivalent:**

- Cloud Files API (`CfGetPlaceholderStateFromFileInfo`)
- Shell extension overlays

### 6. Permissions (`src/permissions.rs`)

**~60 lines**

**What it does on macOS:**

- Checks Full Disk Access permission
- Opens System Settings to Privacy pane

**Linux/Windows:**

- Not needed—different permission models
- Could stub with "always granted" or implement equivalent checks

## Frontend graceful degradation

The frontend already handles missing commands gracefully. All macOS-only Tauri commands are wrapped with try/catch and
return sensible defaults:

```typescript
export async function listVolumes(): Promise<VolumeInfo[]> {
    try {
        return await invoke<VolumeInfo[]>('list_volumes')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}
```

This means the app will launch and work on Linux/Windows, just with reduced functionality.

## Dependencies by platform

### macOS-only Cargo dependencies (in `Cargo.toml`)

```toml
[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10.1"
core-services = "1.0.0"
icns = "0.3.1"
plist = "1.8.0"
urlencoding = "2.1.3"
objc2 = { version = "0.6", features = ["std"] }
objc2-foundation = { ... }
objc2-app-kit = { ... }
smb = "0.11.1"
smb-rpc = "=0.11.1"
chrono = "0.4"
security-framework = "3.2"
```

### Potential cross-platform alternatives

| macOS crate          | Cross-platform alternative                       |
|----------------------|--------------------------------------------------|
| `file_icon_provider` | `freedesktop-icons` (Linux), Windows Shell API   |
| `smb` / `smb-rpc`    | `libsmbclient` bindings or custom implementation |
| `security-framework` | `keyring` crate (cross-platform)                 |
| `objc2-*`            | N/A (macOS-specific by nature)                   |

## Recommended implementation phases

### Phase 1: Basic Linux support (get it running)

- Verify app launches and file browsing works
- Stub remaining macOS commands if needed
- Basic icon support (file type icons, no custom app icons)
- Test copy/move/delete operations

### Phase 2: Linux volume discovery

- Implement Linux-specific volume listing
- XDG user directories
- Removable media detection

### Phase 3: Linux icons

- XDG icon theme integration
- MIME type icon mapping

### Phase 4: Linux network/SMB (optional for initial release)

- Avahi for host discovery
- SMB share listing (new library needed)
- Secret Service for credentials

### Windows follows same phases

With additional considerations:

- Drive letter handling
- Path separator differences (mostly handled by `std::path`)
- Windows-specific APIs

## Testing considerations

Currently, macOS-only tests are gated with `#[cfg(target_os = "macos")]`. For cross-platform:

1. Add Linux/Windows CI runners
2. Create platform-specific test modules
3. Consider Docker-based testing for SMB (already have docs for this)

## E2E testing on Linux

Unlike macOS, Linux has WebDriver support via `webkit2gtk-driver`. This means:

- Playwright/WebdriverIO E2E tests can run on Linux
- Could run E2E in CI on Linux even if primary target is macOS
- Catches rendering and interaction bugs

See `_ignored/crab-nebula-integration-tauri.md` for CrabNebula's approach to Tauri E2E testing.

## References

- [Tauri platform differences](https://tauri.app/develop/)
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
- [freedesktop-icons crate](https://crates.io/crates/freedesktop-icons)
- [keyring crate](https://crates.io/crates/keyring) - cross-platform credential storage
