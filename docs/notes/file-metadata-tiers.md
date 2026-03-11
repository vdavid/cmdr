# File metadata scope and cost tiers

When displaying files in the explorer, each piece of metadata has different performance characteristics. With 50k+ file
directories, we must be deliberate about what to fetch eagerly vs. on-demand.

## Tier 1: Free (from single `stat()` call, already performed)

| Field         | Source                               |
| ------------- | ------------------------------------ |
| Name          | `DirEntry::file_name()`              |
| Size          | `metadata.len()`                     |
| Is directory  | `metadata.is_dir()`                  |
| Modified date | `metadata.modified()`                |
| Created date  | `MetadataExt::st_birthtime()`        |
| Permissions   | `metadata.permissions().mode()`      |
| Owner uid/gid | `MetadataExt::st_uid()` / `st_gid()` |
| Is symlink    | `metadata.is_symlink()`              |

## Tier 2: Cheap (extra syscall, cacheable)

| Field          | How to get                        | Cost            |
| -------------- | --------------------------------- | --------------- |
| Owner name     | `users` crate to resolve uid→name | ~1μs, cacheable |
| Symlink target | `std::fs::read_link()`            | ~1μs if symlink |

## Tier 3: macOS-specific (requires Objective-C APIs, ~50-100μs/file)

Added date, last opened date, locked flag, stationery pad flag, kind (localized), cloud sync status.
Use Spotlight / `NSURL resourceValuesForKeys:` or xattrs.

## Tier 4: Extended/content-based (1-100ms+, reads file content)

EXIF/media metadata, PDF metadata, audio/video metadata.

## Current scope

**Included in list view (Tier 1-2)**: All Tier 1 fields (zero extra cost) + owner name (cached uid→name resolution).

**Deferred (Tier 3-4)**: Added/opened dates (Spotlight-dependent, unreliable), locked/stationery flags (rarely used),
kind (can derive from extension on frontend), EXIF and media metadata (on-demand only).

**Future work**: Cloud sync status (iCloud, Dropbox, GDrive) — valuable, requires xattr reads.
