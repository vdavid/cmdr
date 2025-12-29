# Dropbox sync status detection on macOS

Dropbox does not document how to programmatically detect file sync status. This document captures our
reverse-engineering findings from December 2025.

## Overview

On macOS 12.5+, Dropbox uses Apple's **File Provider API**. Files live in `~/Library/CloudStorage/Dropbox/` (with
`~/Dropbox` as a symlink). Sync status can be detected via:

1. **File system metadata** (`stat()`) - fast, no API calls
2. **Spotlight metadata** (`mdls` / `NSMetadataItem`) - slower but more detailed
3. **Extended attributes** (`com.dropbox.attrs`) - undocumented binary format

---

## Detection methods

### Method 1: stat() - fastest

The `stat()` system call provides two key indicators:

| Field       | Access in Rust                            | Meaning                             |
| ----------- | ----------------------------------------- | ----------------------------------- |
| `st_blocks` | `metadata.blocks()`                       | Number of 512-byte blocks allocated |
| `st_flags`  | `metadata.st_flags()` (via `MetadataExt`) | File flags including `SF_DATALESS`  |

**Key insight**: Online-only files have `blocks = 0` and `SF_DATALESS` flag set.

```rust
use std::os::unix::fs::MetadataExt;

const SF_DATALESS: u32 = 0x40000000;

fn is_online_only(metadata: &std::fs::Metadata) -> bool {
    metadata.blocks() == 0 || (metadata.st_flags() & SF_DATALESS != 0)
}
```

### Method 2: Spotlight metadata - for sync progress

Spotlight indexes Dropbox files with these relevant keys:

| Key                    | Type       | Meaning                                        |
| ---------------------- | ---------- | ---------------------------------------------- |
| `kMDItemIsUploaded`    | Bool (0/1) | File content fully uploaded to Dropbox servers |
| `kMDItemIsUploading`   | Bool (0/1) | File is currently uploading                    |
| `kMDItemIsDownloading` | Bool (0/1) | File is currently downloading                  |
| `kMDItemIsDownloaded`  | Bool       | File content fully downloaded (often null)     |

**Note**: Spotlight indexing may lag behind actual state. New/temporary files may show `(null)` for all values.

Query via command line:

```bash
mdls -name kMDItemIsUploaded -name kMDItemIsUploading /path/to/file
```

### Method 3: Extended attributes - undocumented

Dropbox stores sync metadata in `com.dropbox.attrs` (26 bytes, binary format):

```bash
xattr -px com.dropbox.attrs /path/to/file
# Output: 0A 12 0A 10 FB 39 37 41 16 17 ED F2 00 00 00 00 00 8E E5 7E 10 ...
```

The format appears to be a protobuf or custom binary encoding. First 16 bytes are consistent across files (possibly
account/folder ID). Last 10 bytes vary by file/state. **Not recommended** for detection due to lack of documentation.

---

## Observed file states

### Synced (downloaded and up-to-date)

```
stat:  blocks=224, flags=0x40 (64)
mdls:  kMDItemIsUploaded=1, kMDItemIsUploading=0
Finder: Green checkmark ✅
```

### Online-only (cloud stub)

```
stat:  blocks=0, flags=0x40000060 (1073741920)
       Flags include: SF_DATALESS, UF_COMPRESSED, UF_TRACKED
mdls:  kMDItemIsUploaded=1, kMDItemIsUploading=0
Finder: Cloud icon ☁️
```

### Uploading (syncing to cloud)

```
stat:  blocks>0, flags=0x40 (64)
mdls:  kMDItemIsUploaded=0, kMDItemIsUploading=1
Finder: Circular arrows 🔄
```

### Downloading (syncing from cloud)

```
stat:  blocks=0, flags=0x40000060 (SF_DATALESS)
       Size shows expected final size, blocks still 0
mdls:  All values may be (null) during download
Finder: Pie chart progress indicator 🥧
```

### Download progress calculation

Finder shows a pie-chart progress indicator during downloads. We can potentially calculate progress using:

```
progress = (blocks * 512) / size
```

Where:

- `blocks` = `stat().st_blocks` (number of 512-byte blocks allocated)
- `size` = `stat().st_size` (total file size)

**Caveat**: In our testing, `blocks` remained 0 during download (File Provider may buffer to temp location). This
approach may not work with Dropbox's File Provider implementation. There is no public API to query per-file download
progress from a third-party File Provider extension – the `NSProgress` is internal to Dropbox's extension.

**Recommendation**: Show syncing icon 🔄 without progress percentage. Displaying accurate progress would require
reverse- engineering Dropbox's internal progress mechanism, which is not feasible.

---

## Recommended detection algorithm

```rust
pub enum DropboxSyncStatus {
    Synced,        // Fully downloaded and uploaded
    OnlineOnly,    // Stub file, content in cloud
    Uploading,     // Local changes being uploaded
    Downloading,   // Cloud content being downloaded
    Unknown,       // Not a Dropbox file or status unclear
}

pub fn get_sync_status(path: &Path) -> DropboxSyncStatus {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return DropboxSyncStatus::Unknown,
    };

    let blocks = metadata.blocks();
    let flags = metadata.st_flags();
    let is_dataless = (flags & 0x40000000) != 0 || blocks == 0;

    // Fast path: check stat() first
    if is_dataless {
        // Could be online-only OR downloading
        // To distinguish, would need Spotlight query (slower)
        return DropboxSyncStatus::OnlineOnly;
    }

    // File has local content - check if uploading via Spotlight
    // (This part requires NSMetadataItem query - omitted for brevity)
    // if is_uploading { return DropboxSyncStatus::Uploading; }

    DropboxSyncStatus::Synced
}
```

---

## File flags reference

Relevant macOS file flags from `<sys/stat.h>`:

| Flag            | Value      | Meaning                                |
| --------------- | ---------- | -------------------------------------- |
| `UF_NODUMP`     | 0x00000001 | Don't dump file                        |
| `UF_COMPRESSED` | 0x00000020 | File is compressed                     |
| `UF_TRACKED`    | 0x00000040 | File changes tracked (File Provider)   |
| `UF_DATAVAULT`  | 0x00000080 | Entitlement required for access        |
| `SF_DATALESS`   | 0x40000000 | **File is a stub** - content not local |

Online-only Dropbox files typically have flags: `SF_DATALESS | UF_COMPRESSED | UF_TRACKED` = `0x40000060`

---

## Open questions

1. **Downloading vs online-only**: Both show `blocks=0` and `SF_DATALESS`. During active download, Spotlight metadata
   may be `(null)`. Need to find a reliable way to distinguish "static online-only" from "currently downloading".

2. **Sync errors**: How are sync errors (conflicts, permission issues) indicated? Need test files with error states.

3. **Ignored files**: Files with `com.dropbox.ignored` xattr are not synced. Detection is straightforward:

    ```bash
    xattr -p com.dropbox.ignored /path/to/file  # Returns "1" if ignored
    ```

4. **Shared folders**: Do shared folder files have different status indicators?

5. **Spotlight latency**: How quickly does Spotlight update after state changes? Is there a faster notification
   mechanism?

---

## Test files used

All in `~/Dropbox/sharing/`:

| File                     | State       | Notes                               |
| ------------------------ | ----------- | ----------------------------------- |
| `edlevo-downloaded.png`  | Synced      | blocks=224, flags=0x40              |
| `edlevo-online-only.png` | Online-only | blocks=0, flags=0x40000060          |
| `edlevo-syncing.png`     | Uploading   | blocks>0, kMDItemIsUploading=1      |
| `.../tmp.a`              | Downloading | blocks=0, size=571MB, mdls all null |

---

## References

- Apple File Provider documentation: https://developer.apple.com/documentation/fileprovider
- Dropbox help article on sync icons: https://help.dropbox.com/sync/icons
- macOS `stat.h` flags: `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/sys/stat.h`
