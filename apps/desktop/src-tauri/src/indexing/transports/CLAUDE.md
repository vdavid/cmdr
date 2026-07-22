# Indexing transports

Per-transport enable + live-watch wiring. Each transport builds on the shared machinery (the `network_scanner` trait
BFS for SMB/MTP, the local `scanner` + `watch` pipeline for local-external) and differs only in HOW a volume is enabled
and HOW live changes arrive.

## Must-knows

- **SMB/MTP index only over a `direct` (smb2/PTP) session; an `os_mount` SMB share upgrades first.** Every SMB refusal
  is a TYPED `SmbIndexGateReason` (`NotRegistered` / `NotAnSmbVolume` / `UpgradeFailed` / `CredentialsNeeded` /
  `Disconnected`) crossing IPC as a snake_case tag, NEVER a message substring. MTP has no gate (USB is FDA-independent).
- **Live watch runs with NO pane open.** `apply_smb_change` hooks BEFORE the pane-listing early-return (the watcher's
  lifetime is the volume's, not a pane's). MTP's `mtp_watch` feeds off the PTP event loop the same way.
- **Deletes resolve against the INDEX, never a live stat.** A `Removed` / `ObjectRemoved` for a name/handle the index
  never had is a no-op; SMB/MTP do not round-trip the device per delete. MTP resolves removals via the object handle
  stored in the `inode` column (`find_entry_by_inode`), since PTP `ObjectRemoved` carries only an opaque handle.
- **MTP gates BEFORE resolving** (`buffer_mtp_handle_if_scanning`): during a scan it buffers the RAW handle (zero device
  I/O); resolving ahead of the scanning check timed out on the contended device (the livelock).
- **Reconnect AUTO-RESUMES an SMB index only when a scan completed AND `user_disabled` is unset**
  (`resume_smb_index_if_enabled`, gated on the PERSISTED state, never the live registry). The sticky `user_disabled`
  marker is written ONLY at the explicit disable command, NEVER inside `stop_indexing` (which also runs on eject,
  unmount, an interrupted scan, and the memory watchdog).
- **FAT/exFAT `LocalExternal` drives store `inode: None`** (via `IndexPathSpace::trust_inode`): a reused derived inode
  false-matches the local rename pre-pass and corrupts `dir_stats`. `classify` decides local-external vs SMB-fall-through
  from TYPED facts (a live smb2 session, or a network fs-type), never a volume-id/path substring.
- **FSKit stop-before-unmount (2026-07-15 incident): stop a `LocalExternal` index BEFORE its volume unmounts.** An open
  FSEvents stream / SQLite handle at unmount can wedge the userspace FSKit service and kernel-panic the machine. The
  eject-stop ORDERING is the only reliable defense; test with synthetic disk images ONLY.

## Module map

Sub-subdirs do NOT get their own docs; they're covered here.

- `smb/` — `index.rs` (the direct-smb2 gate + auto-resume), `watch.rs` (`CHANGE_NOTIFY` → index via `apply_smb_change`,
  and `index_relative_path`, the shared mount-strip), `integration_test.rs`.
- `mtp/` — `index.rs` (enable, no gate), `watch.rs` (PTP-event live watch, gate-before-resolve, handle→removal).
- `local_external/` — `index.rs` (enable + `classify`; the LOCAL scanner drives a mount-rooted drive).

Owned elsewhere: the `Volume`-trait BFS scanner, scan pacing, NAS system-dir skips, and no-completion-on-empty-root
live in `../network_scanner/CLAUDE.md` and `../reconcile/CLAUDE.md`; the freshness state machine, phase, registry,
and `force_rescan` typed-kind routing in `../lifecycle/CLAUDE.md`; the live-change apply INTO the index and the event
loop in `../watch/CLAUDE.md`; the mount-relative path transforms in `../paths/CLAUDE.md`; retention/eviction in
`../resources/CLAUDE.md`.

SMB, MTP, and local-external enable + live watch: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
