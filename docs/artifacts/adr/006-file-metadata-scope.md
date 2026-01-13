# ADR 006: File metadata scope and cost tiers

## Status

Accepted

## Context

When displaying files in the explorer, we can retrieve various metadata. macOS provides extensive file information, but
each piece has different performance characteristics. With lists of 50k+ files, we must be deliberate about what to
fetch eagerly vs. on-demand.

## Decision

We will categorize metadata into tiers by cost and load accordingly:

### Tier 1: Free (from single `stat()` call, already performed)

| Field         | Source                               | Notes          |
| ------------- | ------------------------------------ | -------------- |
| Name          | `DirEntry::file_name()`              | Already have   |
| Size          | `metadata.len()`                     | Already have   |
| Is directory  | `metadata.is_dir()`                  | Already have   |
| Modified date | `metadata.modified()`                | Already have   |
| Created date  | `MetadataExt::st_birthtime()`        | Same syscall   |
| Permissions   | `metadata.permissions().mode()`      | Unix mode bits |
| Owner uid/gid | `MetadataExt::st_uid()` / `st_gid()` | Same syscall   |
| Is symlink    | `metadata.is_symlink()`              | Same syscall   |

### Tier 2: Cheap (extra syscall, cacheable)

| Field          | How to get                        | Cost            |
| -------------- | --------------------------------- | --------------- |
| Owner name     | `users` crate to resolve uid→name | ~1μs, cacheable |
| Symlink target | `std::fs::read_link()`            | ~1μs if symlink |

### Tier 3: macOS-specific (requires Objective-C APIs)

| Field               | API                                               | Cost                |
| ------------------- | ------------------------------------------------- | ------------------- |
| Added date          | Spotlight / `NSURL resourceValuesForKeys:`        | ~50-100μs/file      |
| Last opened date    | Spotlight / NSURL                                 | Same                |
| Locked flag         | `NSURL` with `NSURLIsUserImmutableKey`            | ~50μs               |
| Stationery pad flag | `NSURL` with `NSURLStationeryKey`                 | Same                |
| Kind (localized)    | `NSURL.localizedTypeDescription`                  | Requires macOS APIs |
| Cloud sync status   | xattrs like `com.apple.icloud.itemDownloadStatus` | ~10μs               |

### Tier 4: Extended/content-based

| Category             | How to get                     | Cost                 |
| -------------------- | ------------------------------ | -------------------- |
| EXIF/media metadata  | `kamadak-exif`, `image` crates | 1-50ms+ (reads file) |
| PDF metadata         | `lopdf` crate                  | 10-100ms+            |
| Audio/video metadata | `lofty` crate                  | 10-100ms+            |

## Chosen scope for initial implementation

**Include in list view (Tier 1-2)**:

- All Tier 1 fields (zero extra cost)
- Owner name (cached uid→name resolution)

**Defer (Tier 3-4)**:

- Added/opened dates (Spotlight-dependent, unreliable)
- Locked/Stationery flags (rarely used)
- Kind (can derive from extension on frontend)
- EXIF and media metadata (on-demand only)

**Future work**:

- Cloud sync status (iCloud, Dropbox, GDrive) - valuable, requires xattr reads

## Consequences

### Positive

- Zero performance regression for current functionality
- Created/permissions/owner cost nothing extra
- Clear path for adding macOS-specific metadata later

### Negative

- Some Finder-equivalent fields not immediately available
- macOS-specific features require platform-gated code
