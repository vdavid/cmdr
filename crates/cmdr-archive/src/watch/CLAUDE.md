# Archive live content watch

Refreshes any open listing inside an archive when the backing `.zip` changes on disk (an editor rewriting it, a `cp`
over it, the host's own mutation's final rename). The watch handle lives on the [`ArchiveVolume`](../volume.rs); this
module is the OS watch + event filter behind it.

Depth, the remote-no-watch decision, and the test list: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.

## Must-knows

- **Watch the parent DIRECTORY, not the file.** A safe-overwrite (editor, `cp`, or this crate's own temp+rename)
  replaces the file's inode, so a `notify` watch pinned to the file goes silent after the swap. The directory inode is
  stable; filter the child events down to the archive file (`event_path_targets_archive`, on firmlink-normalized paths).
- **Refresh via the host's `refresh_archive_listings` seam with the PARENT DRIVE id + full `/…/foo.zip/inner` path,
  never the archive id or `directory_changed`.** The listing cache keys archive listings on the parent drive id; feeding
  an archive-inner path (not a real FS path) to `directory_changed` would run a meaningless drive-index sync.
- **Off the executor**: the debouncer callback runs on notify-rs's own thread (no Tokio runtime), so it spawns through
  `host.runtime()`, never `tokio::spawn` (which inherits an ambient runtime and would panic).
- **Local only**: a REMOTE parent has no local path for `notify`, so `start_watch` returns `None` and
  `listing_watch_coverage` stays `None` — freshness is "as of last read". See `DETAILS.md`.
