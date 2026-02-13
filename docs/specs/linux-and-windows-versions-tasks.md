# Linux and Windows versions—tasks

This tasklist accompanies [the spec](linux-and-windows-versions.md).

## Summary

| Platform | Basic (usable) | Full feature parity |
|----------|----------------|---------------------|
| Linux    | 10-15 days     | 30-40 days          |
| Windows  | 15-20 days     | 40-50 days          |

## Phase 1: Basic Linux support

**Goal:** App launches, file browsing works, copy/move/delete works.

**Estimate: 10–15 days**

### Verification and fixes (3–5 days)

- [ ] Set up Linux dev environment (Ubuntu/Fedora VM or WSL2)
- [ ] Build and run app on Linux, document any build issues
- [ ] Verify file listing works for `/home` and other directories
- [ ] Verify sorting, filtering, hidden files toggle work
- [ ] Verify copy/move/delete operations complete successfully
- [ ] Verify file watching triggers UI updates
- [ ] Fix any runtime errors or panics
- [ ] Ensure all macOS-only commands return graceful fallbacks

### Basic icon support (3–4 days)

- [ ] Research `freedesktop-icons` or `gio` icon lookup
- [ ] Implement basic file type icon mapping (folder, file, symlink)
- [ ] Use generic icons for unknown types
- [ ] Test icon loading performance

### CI and testing (2–3 days)

- [ ] Add Linux runner to GitHub Actions
- [ ] Ensure `cargo check` and `cargo test` pass on Linux
- [ ] Verify Svelte tests run on Linux
- [ ] Consider adding webkit2gtk-driver E2E tests

### Documentation (1–2 days)

- [ ] Document Linux build prerequisites (webkit2gtk-dev, etc.)
- [ ] Update README with Linux instructions
- [ ] Note known limitations (no network browser, basic icons)

---

## Phase 2: Linux volume discovery

**Goal:** Sidebar shows user directories, mounted volumes, removable media.

**Estimate: 5–7 days**

### XDG user directories (1-2 days)

- [ ] Parse `~/.config/user-dirs.dirs` for standard locations
- [ ] Map XDG dirs to sidebar items (Desktop, Documents, Downloads, etc.)
- [ ] Handle missing or non-standard configurations

### Mount point discovery (2–3 days)

- [ ] Parse `/etc/mtab` or `/proc/mounts` for mounted filesystems
- [ ] Filter to relevant mount points (exclude system mounts like `/sys`, `/proc`)
- [ ] Detect removable media in `/media/$USER` and `/mnt`
- [ ] Get volume labels where available

### Volume change watching (1–2 days)

- [ ] Watch `/etc/mtab` for changes via inotify
- [ ] Emit volume-changed events to frontend
- [ ] Handle USB drive plug/unplug

### Optional: udisks2 integration (2-3 days extra)

- [ ] Use `udisks2` D-Bus API for better metadata
- [ ] Get drive type, size, label more reliably
- [ ] Eject functionality for removable drives

---

## Phase 3: Linux icons

**Goal:** Proper themed icons for files, folders, and applications.

**Estimate: 5–8 days**

### XDG icon theme integration (3-4 days)

- [ ] Integrate `freedesktop-icons` crate or implement lookup
- [ ] Resolve current icon theme (from GTK settings or environment)
- [ ] Look up icons by MIME type
- [ ] Implement icon size variants (16, 24, 32, 48, 64)

### Application icons (2–3 days)

- [ ] Parse `.desktop` files to find app icons
- [ ] Look up icons in theme or absolute paths
- [ ] Cache resolved icon paths

### Performance optimization (1 day)

- [ ] Benchmark icon lookup performance
- [ ] Add caching layer if needed
- [ ] Consider async icon loading for large directories

---

## Phase 4: Linux network/SMB (optional)

**Goal:** Browse network shares like on macOS.

**Estimate: 15–20 days**

### Host discovery via Avahi (3-4 days)

- [ ] Research Avahi D-Bus API or `avahi-browse` CLI
- [ ] Implement mDNS service browser for `_smb._tcp`
- [ ] Map discovered services to NetworkHost struct
- [ ] Handle service removal events

### SMB share listing (5-7 days)

- [ ] Research SMB options: `libsmbclient` bindings, `smbclient` CLI, or pure Rust
- [ ] Implement share enumeration for a given host
- [ ] Handle authentication (guest, user/pass)
- [ ] Parse share types (disk, printer, IPC)

### Credential storage (2-3 days)

- [ ] Integrate `keyring` crate or `libsecret` directly
- [ ] Store/retrieve SMB credentials per server
- [ ] Handle credential prompts in UI

### Share mounting (3–4 days)

- [ ] Research `mount.cifs` or `gio mount` approaches
- [ ] Implement mount with credentials
- [ ] Handle mount errors and permission issues
- [ ] Unmount on disconnect

### Testing (2–3 days)

- [ ] Set up SMB test containers (docs already exist)
- [ ] Test against Samba, Windows, macOS SMB servers
- [ ] Test authentication edge cases

---

## Phase 5: Windows support

**Estimate: 15–20 days for basic, 40–50 days for full**

### Basic Windows support (15–20 days)

#### Build and verification (5–7 days)

- [ ] Set up Windows dev environment
- [ ] Fix any Windows-specific build issues
- [ ] Verify path handling with drive letters
- [ ] Test basic file operations
- [ ] Handle Windows-specific path edge cases (UNC paths, long paths)

#### Windows icons (4-5 days)

- [ ] Use `SHGetFileInfo` or `IShellItemImageFactory`
- [ ] Extract icons from executables
- [ ] Handle icon caching

#### Volume discovery (4-5 days)

- [ ] Enumerate drives via `GetLogicalDrives()`
- [ ] Get drive labels and types
- [ ] Known folders via `SHGetKnownFolderPath()`
- [ ] Drive change notifications

#### CI (2-3 days)

- [ ] Add Windows runner to GitHub Actions
- [ ] Handle code signing for Windows builds

### Full Windows support (additional 25–30 days)

#### Windows network/SMB (15-18 days)

- [ ] WNet APIs for network enumeration
- [ ] WNetAddConnection2 for mounting
- [ ] Windows Credential Manager integration
- [ ] Network discovery (WSD, NetBIOS)

#### Windows-specific polish (5–7 days)

- [ ] Show in Explorer (`explorer /select,`)
- [ ] Windows preview handlers
- [ ] Properties dialog
- [ ] Jump-list integration
- [ ] Taskbar progress

#### Windows installer (3–5 days)

- [ ] MSI or MSIX packaging
- [ ] Auto-updater for Windows
- [ ] Windows Store considerations

---

## Quick wins (can do anytime)

These are small tasks that improve cross-platform support without major effort:

- [ ] Add platform detection to show/hide macOS-only UI elements (1 day)
- [ ] Implement `show_in_finder` for Linux using `xdg-open` (0.5 days)
- [ ] Implement `show_in_finder` for Windows using `explorer /select,` (0.5 days)
- [ ] Add platform-specific keyboard shortcuts (Ctrl vs. Cmd) (1 day)
- [ ] Document platform differences in user docs (0.5 days)

---

## Dependencies to evaluate

| Need            | Crate/Library                  | Notes                                   |
|-----------------|--------------------------------|-----------------------------------------|
| Linux icons     | `freedesktop-icons`            | Maintained, simple API                  |
| Linux mounts    | `sys-mount` or manual parsing  | Manual is simpler                       |
| Linux secrets   | `keyring`                      | Cross-platform, uses libsecret on Linux |
| Linux SMB       | `libsmbclient` bindings or CLI | No good pure-Rust option                |
| Windows icons   | `windows` crate                | Official Microsoft bindings             |
| Windows secrets | `keyring`                      | Uses Credential Manager on Windows      |

---

## Risk assessment

| Risk                    | Impact | Mitigation                                          |
|-------------------------|--------|-----------------------------------------------------|
| SMB library gap         | High   | May need to shell out or write bindings             |
| Icon theme complexity   | Medium | Start with basic fallback icons                     |
| Windows path edge cases | Medium | Extensive testing, use `std::path`                  |
| Platform-specific bugs  | Medium | Add platform-specific test suites                   |
| Maintenance burden      | High   | Consider feature flags to disable platform features |

---

## Milestones

### M1: Linux alpha (15 days)

- App runs on Linux
- File browsing works
- Basic icons
- Copy/move/delete works

### M2: Linux beta (30 days)

- Volume sidebar works
- Proper themed icons
- Stable for daily use

### M3: Linux stable (40 days)

- Network/SMB browser (if implementing)
- Feature parity with macOS (except macOS-specific features)
- Polished UX

### M4: Windows alpha (20 days after M1)

- App runs on Windows
- File browsing works
- Basic icons

### M5: Windows stable (50 days after M1)

- Full feature parity with Linux
- Windows-specific polish
- Installer and distribution

## References

- [Linux spec](linux-and-windows-versions.md)
- [Tauri cross-platform guide](https://tauri.app/develop/)
- [freedesktop-icons](https://crates.io/crates/freedesktop-icons)
- [keyring crate](https://crates.io/crates/keyring)
- [windows crate](https://crates.io/crates/windows)
