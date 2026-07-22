# Archive live content watch — details

Pull-tier docs for the live content watch. Must-know invariants live in [CLAUDE.md](CLAUDE.md). Read this before any
non-trivial work here: editing, planning, reorganizing, or advising.

An external edit to the backing `.zip` (an editor rewriting it, a `cp` over it, this app's mutation's final rename)
refreshes any open listing inside the archive. The watch lives on the [`ArchiveVolume`](../volume.rs).

## Watch the parent directory, not the file

macOS editors and every safe-overwrite (including this app's own temp+rename mutation) replace the file's inode: write
`foo.zip.tmp`, then atomically rename over `foo.zip`. A `notify` watch pinned to the OLD inode goes silent after such a
swap. So `start_watch` watches the archive's PARENT DIRECTORY (`RecursiveMode::NonRecursive`) — the directory inode is
stable across the swap, so no re-arming is needed — and filters the directory's child events down to the archive file
(`event_path_targets_archive`). The filter compares on the firmlink-normalized forms
(`indexing::paths::firmlinks::normalize_path`), the same rebasing `file_system::watcher` does, because FSEvents reports
canonical `/private/tmp/…` paths while the archive path is the user-navigated `/tmp/…` form. This mirrors the local
listing watcher's own parent-directory-non-recursive shape.

## Notification identity: parent drive id + full path, never the archive id

The listing cache keys archive listings on the PARENT DRIVE id plus the full `/…/foo.zip/inner` path (see
[`../DETAILS.md`](../DETAILS.md) § "Routing and lifecycle"). On a matching event the callback drops the stale parsed
index (`cache.clear()`) and calls `caching::refresh_archive_listings(parent_drive_id, archive_path)`, which finds every
open listing at or inside the archive path (`Path::starts_with`, component-wise — so `/a/foo.zip` matches the root and
inner listings but not a `/a/foo.zipper` sibling or the containing `/a`) and re-reads each through
`notify_full_refresh`. That re-resolves `(parent_id, inner_path)` back to this `ArchiveVolume`, so an LRU-evicted archive
re-registers lazily. It deliberately does NOT go through `notify_directory_changed`: that runs the drive-index sync
(`apply_smb_change`) up front, and an archive-inner path isn't a real filesystem path, so feeding it to the index would
be meaningless.

## Cache invalidation

The `(path, size, mtime)` key already misses after any edit, so `cache.clear()` isn't needed for correctness — but it
releases the old `Arc<ArchiveIndex>` instead of leaking one parsed index per edit. Clearing runs in the (synchronous)
`notify` callback before the async refresh spawns, so the re-read re-parses the new bytes.

## Off the executor

The debouncer callback runs on notify-rs's own thread, which has no Tokio runtime, so it uses
`tauri::async_runtime::spawn` (never `tokio::spawn`, which would panic) — the same rule as `file_system::watcher`.

## Mid-write safety (keep the old listing)

A writer mid-rewrite leaves a truncated central directory. On such a refresh, `list_directory` errors
(`ArchiveError::Corrupt`/`NotAnArchive` → `VolumeError`), and `notify_full_refresh` returns early WITHOUT touching the
cache — so the pane keeps its last-good entries rather than blanking, and the next event (when the write settles)
retries. The damaged-archive banner is produced only at NAVIGATION time (the listing seam; see [`../DETAILS.md`](../DETAILS.md)
§ "Decision: typed `ArchiveError → VolumeError` mapping"), never from this refresh path, so a transient mid-write never
flashes an error. Pinned by `a_truncated_midwrite_archive_keeps_the_previous_listing`.

## Lifecycle (leak-free)

`VolumeManager::resolve` starts the watch exactly once, gated on the `register_if_absent` winner, so repeated resolves of
an already-registered archive don't churn watchers. The `ArchiveContentWatch` handle lives on the `ArchiveVolume`; when
the archive LRU evicts the volume (`unregister` drops the registry's `Arc`) or the app tears down, the handle drops and
the `Debouncer`'s own `Drop` stops the OS watch. A held `Arc` (an in-flight read) keeps the watch alive until the last
reference drops — harmless (a re-resolve after eviction starts a fresh watch; two briefly overlap, both fire idempotent
refreshes). `active_watch_count` (incremented on start, decremented in the handle's `Drop`) lets
`lru_eviction_releases_the_archive_and_its_watch` prove eviction leaks no watcher. `listing_is_watched` is `true` only
while the handle is present, so a listing never claims freshness the backend can't back; if the watch fails to establish
(`notify` refuses the path — e.g. a non-local parent), it stays `false`.

## Decision: remote archives have NO live watch — freshness is "as of last read"

The content watch is a LOCAL `notify` watch on the backing `.zip`'s parent directory. A REMOTE parent (direct SMB / MTP)
has no local path for `notify` to watch, so `start_watch` returns `None` and a remote `ArchiveVolume`'s
`listing_is_watched` is permanently `false`. Two consequences, both correct-by-construction rather than a gap to close:

- **The write-op fresh-listing oracle never serves a remote archive listing from cache.** `listing_is_watched == false`
  means every pre-flight scan of a remote archive re-reads it honestly (and `try_get_watched_listing` also guards a
  remote archive-inner path explicitly — see `volume/CLAUDE.md` § `resolve`). So a copy/delete inside a remote archive
  always sizes against a fresh parse, never a stale cache.
- **Push-refresh for an EXTERNAL edit of a remote `.zip`: SMB yes, MTP no.** SMB: the recursive share watcher
  (`smb_watcher.rs`) already receives a `CHANGE_NOTIFY` for any changed `.zip` on the share, so its Modified/Renamed
  handlers ALSO call `caching::refresh_archive_listings` for a supported-archive path, pushing an out-of-band edit to
  any open inner listing. That refresh is a SEPARATE, visible-listing-only consumer from this `listing_is_watched`
  oracle: the flag stays `false` for a remote parent regardless (the SMB watcher is lossy under load, so the write-op
  oracle must keep re-reading pre-flight scans honestly — see `backends/DETAILS.md` § "SMB archive push-refresh" for the
  mechanism and `backends/archive/volume_test.rs::remote_backed_archive_never_reports_listing_is_watched` pinning it).
  MTP: nothing forwards an out-of-band change to an open inner listing (MTP's `ObjectInfoChanged` is absent on many
  devices and hooking it isn't a clean few-liner), so an MTP-backed archive pane shows the zip as of its last read until
  the user re-navigates or refreshes (F5). For both, the app's OWN edit refreshes the pane through the normal
  listing-cache path (the edit's driver invalidates the `(parent_id, archive_path)` listing on completion), and the
  `(path, size, mtime)` cache key forces a re-parse on the next read, so a stale render can never outlive a navigation.

**Interaction with mutation.** A zip edit's FINAL atomic rename over `foo.zip` is a change event this watch catches —
that IS the desired post-edit refresh. A concurrent browse in the other pane reading the archive mid-edit sees either
the old-complete or the new-complete file, never a torn read, because the rename is atomic on one filesystem: the
reader's `LocalFileSource` opens whichever inode the rename has published. The read isn't serialized on the edit's lane,
but it doesn't need to be — atomicity, not lane serialization, is what prevents the torn read. The `(path, size, mtime)`
key plus this watch's `cache.clear()` guarantee the post-rename read re-parses the new file.

## Testing

The pure event filter (`event_path_targets_archive`: exact match, the `/private` firmlink normalization, sibling and
prefix-similar rejection) is unit-tested inline. `watch_integration_test.rs` drives the whole refresh through
`VolumeManager::resolve` + `LISTING_CACHE` against real temp zips: an on-disk edit reflected in the listing while an
outside listing is left untouched (scoping), a truncated mid-write keeping the previous listing, the two real-notify
end-to-end refresh tests (an in-place rewrite and a temp+rename inode swap), and LRU eviction releasing the archive's
watch (`Arc::strong_count` drops to the test's own after eviction).

The two real-notify tests are **self-healing** under a saturated suite, not retry-dependent:
`drive_refresh_until` redoes the zip rewrite until the watch delivers a refresh and the new entry lands in the listing,
all inside one 15 s budget. This defeats both the just-registered-watch arming window (a mutation landing before macOS
finishes arming FSEvents is dropped outright, not delayed) and a lone coalesced/dropped event when every core is busy —
both unrecoverable by waiting. It mirrors `downloads::watcher::observe_mutation`; the shared `real-notify` nextest group
(serialized, `retries = 0`) lives in [`.config/nextest.toml`](../../../../../../../../../.config/nextest.toml).
