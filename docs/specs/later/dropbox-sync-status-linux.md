# Dropbox sync status on Linux

Not started. On Linux the pane draws no cloud badges at all: `get_sync_status` has a non-macOS arm that returns an empty
map (`apps/desktop/src-tauri/src/commands/sync_status.rs`), and `file_system::sync_status` is gated
`#[cfg(target_os = "macos")]` in `file_system/mod.rs`.

This doc holds the protocol research, which is the part that took the work, plus what the current module shape means for
anyone building the Linux arm. ❗ The step-by-step file plan this doc once carried described a single
`file_system/sync_status.rs`; that module is now a six-file directory with its own concurrency design, so the shape
below replaces it.

Linux support isn't advertised and this gap isn't on the ledger in `docs/notes/linux-gaps-2026-08-10.md`, which records
what a Linux user hits today. Nothing blocks on this.

## What the Linux arm can and can't cover

macOS answers for every provider at once: Dropbox, iCloud Drive, Google Drive, OneDrive, and Box all register File
Provider domains, and one `NSURL` resource value covers them all. **Linux has no such common layer.** Dropbox exposes
its own Unix socket, and every other provider would need its own integration, so a Linux arm is a Dropbox arm, and the
badge stays absent everywhere else.

Linux Dropbox dropped Smart Sync in 2019, so `OnlineOnly` and `Downloading` can't occur. Only `Synced`, `Uploading`
(mapped from "syncing"), and `Unknown` are reachable.

## The protocol

Dropbox for Linux answers sync-status questions on a Unix domain socket at `~/.dropbox/command_socket`. This is the same
protocol the Nautilus Dropbox extension uses.

- **Request**: `icon_overlay_file_status\npath\t<filepath>\ndone\n`
- **Response**: `status\t<status_string>\ndone\n`
- **Batching**: one connection serves every path in a batch. Clone the stream for a separate reader and writer so
  `BufReader` doesn't swallow bytes belonging to the next response.

Status strings map as:

- **"up to date"**: `Synced`. Matches the cloud.
- **"syncing"**: `Uploading`. The protocol carries no direction, and with no Smart Sync on Linux it's always an upload.
- **"unsyncable"**: `Unknown`. Can't sync (permissions, path length).
- **"unwatched"**: `Unknown`. Outside the Dropbox folder.

**CLI fallback**: `dropbox filestatus <path>` prints `<path>: <status>`. Parse with `rfind(": ")` so a colon inside a
path doesn't split the line in the wrong place. One subprocess per path, so it's a fallback and never the default.

Neither path needs a new dependency: `std::os::unix::net::UnixStream`, `dirs`, and `log` are all already in the tree.

## What the current module means for the build

`file_system/sync_status/` is not a single function any more. Read its `CLAUDE.md` and `DETAILS.md` before designing the
Linux arm; the questions below are the ones that shape it.

- **The public API is `statuses_within(paths, deadline) -> (HashMap<String, SyncStatus>, bool)`**, plus
  `status_within_blocking`, `invalidate_dir`, and `invalidate_path`. A Linux arm has to answer the same shape, deadline
  and all, or the caller changes too.
- **`SyncStatus` lives in `sync_status/mod.rs`**, next to the private `SyncKnowledge` the cache stores. Making the enum
  cross-platform means lifting it out of a macOS-gated module, and `SyncKnowledge` has to come along or be split,
  because the cache is keyed on it.
- **Reuse `cache.rs` and `service.rs`; skip `pool.rs`.** The cache (per-directory, TTL'd per knowledge kind) and the
  service (one batch in flight, superseded rather than stacked) exist because a pane re-asks for every visible path
  several times a second, which is just as true on Linux. The 8 MB-stack thread pool exists because a macOS framework
  call blows rayon's 2 MB stacks and can block forever inside `fileproviderd`; a local socket round-trip is neither, so
  it doesn't earn the pool.
- **There is no Linux equivalent of tier one.** The macOS probe answers "no provider owns this file" from one xattr read
  (`cmdr_fs::file_provider`, `com.apple.file-provider-domain-id`, 13.9 µs), and nearly every file on the machine stops
  there. Linux has no such marker, so the arm needs its own cheap ancestor gate (the Dropbox folder root, read once)
  before it pays a socket round-trip per path. ⚠️ Without that gate every listing in every directory talks to the
  daemon.
- **The IPC types differ per platform today.** The macOS command returns `TimedOut<HashMap<String, SyncStatus>>` and the
  fallback returns `TimedOut<HashMap<String, String>>`. Widening the `#[cfg]` collapses them to one type, which changes
  `bindings.ts`; see `CONTRIBUTING.md` § Linux testing for why `pnpm check bindings-fresh` can't be trusted to tell you
  so from a Linux host.

## Testing

Parsing is pure and testable without I/O: the status-string mapping (including case and whitespace handling, and unknown
strings) and the CLI line parser with its colon-in-path case.

The socket half tests against a real Unix socket in a `tempfile::TempDir` with a thread standing in for the Dropbox
daemon: one file, two files over one connection, a socket path that doesn't exist (degrades to empty), and an empty
input (returns early). Inject the socket path so the test never depends on `$HOME`.

The CLI fallback needs no integration test of its own: its parsing is covered directly, and the missing-socket test
covers the degradation path.

## Checks

`pnpm check rust-tests clippy rustfmt cfg-gate` covers it. `cfg-gate` is the one that matters most here: it's what
catches a macOS-only import leaking into the Linux build.
