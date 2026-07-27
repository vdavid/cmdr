# Network-volume enrichment

Reading an opted-in SMB volume's image bytes off the wire so a NAS's photos become searchable. The ONE part of the
subsystem with no `importance/` sibling to copy (`importance` never touches a network mount). `mod.rs` is the pass,
`fetch.rs` the byte fetchers, `policy.rs` the conservative gates, `budget.rs` the prefetch admission, `config.rs` the
opt-in / override / exclusion globals, `enrich.rs` the parallel pipeline.

## Must-knows

- **Fetch through the app's OWN transport session first**, OS mount only as fallback (picked per pass by
  `Volume::supports_local_fs_access()`). ❌ Don't reach for plain `std::fs` on `/Volumes/…`: macOS TCC owns the mount
  and hands unsigned dev binaries `EPERM` (reproduced twice, 2026-07-16 — the pass stalled at zero images), and the
  direct path's typed `VolumeError`s are what make pause-vs-skip exact.
- **ONLY a TYPED transport loss pauses a pass** (`DeviceDisconnected` / `ConnectionTimeout`, or the mount path's
  transport-loss errno set / read timeout). Every other per-file error is `FetchError::Unreadable`: skip it, count it,
  write NO row. ❌ Never pause on a per-file fault — it never clears, and the pass stalls forever.
- **A pause is not a failure and never deletes.** A `Paused`/`Cancelled` pass returns BEFORE GC and writes no `Failed`
  row for the in-flight image (`Failed` is reserved for a good read with a bad decode). Completed rows survive; resume
  rides the registration bus on remount.
- **`NotIdle` is TRANSIENT, `Disconnected` and `Cancelled` are not.** `should_retry_when_idle` is `NotIdle` ONLY:
  looping on the others would spin `wait_until_idle_to_resume` against a condition it can't clear.
- **Rows keep the INDEX-relative identity** (`/DCIM/x.jpg`), never the OS path — that's what matches the index + GC set.
  `os_join(mount_root, rel)` reaches the real file; `os_folder_to_index_prefix` is its inverse for OS-keyed config.
- **The "always index" override is load-bearing here.** The production importance oracle yields `None` for network
  volumes, so ONLY override-covered volumes/folders enrich. ❌ Don't treat `None` as enrich-all.
- **Never block a runtime worker.** Both fetchers bound the whole read (`recv_timeout` on a throwaway thread /
  `tokio::time::timeout`) and return `Disconnected` rather than wedging; the fetch happens in the ENRICH layer, so a
  hung transport can never stall another volume's OCR. `MAX_FETCH_BYTES` skips a pathological file instead of OOMing.
- **Prefetch admission is bounded by BYTES, not file count** (`budget.rs`); a count-based queue would buffer gigabytes
  on a RAW-heavy corpus. An over-cap file is admitted alone (never deadlocks); a stop wakes a blocked acquire.
- **`config` is a settings-seeded process global**, not a per-volume store, and `is_excluded` is read LIVE while
  threshold/override stay pass-snapshot. `path_is_within` is trailing-slash-safe, so `/Photos2` isn't within `/Photos`.

The byte-fetch decision, the conservative-fetch policy, resumability, the parallel pipeline, and the network settings
UI: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
