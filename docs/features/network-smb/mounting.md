# Mounting

How Cmdr mounts SMB shares to the local filesystem.

**Related ADR**: [ADR 011: Use NetFSMountURLAsync for SMB mounting](../../artifacts/adr/011-netfs-mount-url-async.md)

## Overview

When a user selects a share (like "TestShare" on "TestServer"), the app:

1. Mounts the share to `/Volumes/<ShareName>` (or similar)
2. Updates the volume selector to show the mounted share
3. Navigates to the mounted path so the user can browse files

## Implementation: `NetFSMountURLAsync`

We use macOS's native `NetFSMountURLAsync` API from **NetFS.framework** for non-blocking, secure mounting.

### API signature (C)

```c
int32_t NetFSMountURLAsync(
    CFURLRef url,                    // smb://server/share
    CFURLRef mountpath,              // /Volumes/ShareName or NULL for auto
    CFStringRef user,                // username or NULL
    CFStringRef passwd,              // password or NULL
    CFMutableDictionaryRef options,  // mount options
    CFMutableDictionaryRef *mountInfo,
    dispatch_queue_t dispatchQueue,  // where callback runs
    NetFSMountURLBlock block         // completion handler
);
```

### Rust implementation approach

1. **Link to NetFS.framework**: Add to Cargo build config
2. **Declare bindings**: NetFS isn't in `objc2-foundation`, so we declare functions manually or use `bindgen`
3. **Handle Core Foundation types**: Use `core-foundation` crate for `CFURLRef`, `CFStringRef`, and others
4. **Completion callback**: Use a Rust closure or channel to receive the result

### Pseudocode

```rust
use core_foundation::url::CFURL;
use core_foundation::string::CFString;

async fn mount_share(
    server: &str,
    share: &str,
    credentials: Option<Credentials>,
) -> Result<PathBuf, MountError> {
    let url = format!("smb://{}/{}", server, share);
    let url_ref = CFURL::from_str(&url);

    let (user, pass) = match credentials {
        Some(creds) => (Some(CFString::new(&creds.username)), Some(CFString::new(&creds.password))),
        None => (None, None), // Guest access
    };

    let (tx, rx) = oneshot::channel();

    unsafe {
        NetFSMountURLAsync(
            url_ref,
            null(),     // Auto-select mount point
            user,
            pass,
            null(),     // Default options
            null(),
            dispatch_get_main_queue(),
            |status, mount_info| {
                if status == 0 {
                    let path = extract_mount_path(mount_info);
                    tx.send(Ok(path));
                } else {
                    tx.send(Err(MountError::from_status(status)));
                }
            }
        );
    }

    rx.await.unwrap()
}
```

## Mount locations

Mounted shares appear in `/Volumes/`:

```
/Volumes/
├── Documents          ← Mounted SMB share
├── Media              ← Another mounted share
└── Macintosh HD       ← Local volume (symlink)
```

If a share with the same name already exists, macOS appends a number: `Documents-1`, `Documents-2`, and so on.

## Integration with volume selector

Once a share is mounted:

1. **Detection**: Our existing volume detection code picks it up (scans `/Volumes/`)
2. **Categorization**: Mark it as `NetworkShare` category (or similar)
3. **Selection**: Automatically select this volume in the pane that initiated the mount
4. **Navigation**: Navigate to the share's root so user can browse

## Error handling

| Error             | User-facing message                   | Action                                       |
| ----------------- | ------------------------------------- | -------------------------------------------- |
| Host unreachable  | "Can't connect to TestServer"         | Suggest checking network connection          |
| Share not found   | "Share 'Documents' not found"         | Suggest checking share name                  |
| Auth required     | (Show login form)                     | See [authentication.md](./authentication.md) |
| Auth failed       | "Invalid username or password"        | Re-show login form                           |
| Permission denied | "You don't have access to this share" | No action available                          |
| Already mounted   | (No error, just navigate)             | Use existing mount                           |

## Loading UX during mount

While mounting, display a loading screen covering the pane:

```
┌─────────────────────────────────────────────────────────────────┐
│ 📁 TestServer / Documents                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ⏳ Mounting "Documents"...                                    │
│   Elapsed: 3 seconds                                            │
│                                                                 │
│   [Cancel]                                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**During loading:**

- UI remains responsive (user can switch to other pane with `Tab`)
- User can go back with `⌘[` or use volume switcher
- Show elapsed time so user knows something is happening

## Timeout handling

Mount operations can hang if the server is slow or unreachable. Use progressive feedback:

| Time      | UI state                                                                           |
| --------- | ---------------------------------------------------------------------------------- |
| 0-10 sec  | "Mounting 'Documents'..." with elapsed counter                                     |
| 10-20 sec | "Still connecting... This is taking longer than expected." + more prominent Cancel |
| 20 sec    | Stop trying. Show error with retry option                                          |

**At timeout (20 sec):**

```
┌─────────────────────────────────────────────────────────────────┐
│ 📁 TestServer / Documents                                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ❌ Couldn't connect to "Documents" on "TestServer"            │
│                                                                 │
│   • Check your network connection                               │
│   • Make sure the server is available                           │
│                                                                 │
│                       [Try again]  [Go back]                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Why not auto-retry indefinitely:**

- Network hiccup: Retry might help, but user should decide
- Server down: Retrying won't help
- Auth failure: Retrying might lock the account!

Let the user explicitly retry when they've fixed something.

## Credential security

When mounting with credentials:

1. Fetch credentials from Keychain immediately before mount call
2. Pass directly to `NetFSMountURLAsync`
3. Don't cache passwords in memory longer than necessary

Keychain fetch is fast (~1-5ms), so fetching just-in-time has no UX impact.

## Unmounting

For completeness, we may want to support unmounting:

```
// Using diskutil
Command::new("diskutil")
    .args(["unmount", "/Volumes/Documents"])
    .status()?;
```

Or via `NetFSUnmountHomeDirectorySync` / related APIs.

## Testing

### Unit tests

- Test URL construction for various server/share combinations
- Test error mapping from NetFS status codes
- Mock NetFS calls for fast tests

### Integration tests

- Mount a test share (if available in test environment)
- Verify mount appears in `/Volumes/`
- Verify cleanup on unmount
