# Incident: cross-volume transfer wedged with no diagnostic trace (2026-07-31)

A user-initiated copy of 764 files (3.10 GB) from a Dropbox File Provider folder to an SMB NAS share stopped dead after
12 files and could not be cancelled, rolled back, or dismissed. The app had to be force-quit, leaving two
byte-incomplete files at their final names on the destination.

**Root cause found on 2026-08-01, in `smb2`. Fixed in smb2 0.15.0.** The client had stopped _sending_: one
`TcpTransport::send` held the transport's write half forever and every later request queued behind that lock, so the
server's "silence" was simply that nobody was asking it anything. Nothing bounded it, because every deadline the crate
had bounds the wait for a _response_ and these requests never reached the wire. See "Resolution" at the bottom.

The rest of this record is preserved unchanged as the forensic yield of the wedge **before** the fix, and as the target
the observability work has to beat. The test for any instrumentation we add is: _would it have answered the open
questions below?_ (Answer, for the record: no. What finally answered them was `nettop` showing zero bytes in **and out**
on a live wedge, next to a log still emitting fresh requests.)

Environment: Cmdr 0.36.2 (prod, `/Applications/Cmdr.app`), macOS 26.5.2, Apple silicon. Source
`~/Library/CloudStorage/Dropbox/Apps/SMSBackupRestore` (volume `root`, Dropbox File Provider domain, 764 files, none
dataless). Destination `/Volumes/naspi/saves/2026-07-31 SMS Backup & Restore save` on share `naspi`
(`smb-192-168-1-111-445-naspi`, `smbConnectionState: direct`). Operation `a8df2e93-be03-40b7-82b4-d72a65e39498`.

## Files here

- `transfer-and-smb.log` - the curated subset (851 lines): `write_operations`, the SMB backend, `smb2::client`, and
  `op_manager`. Read this one first.
- `cmdr.log.slice.gz` - everything the prod log holds for the whole run window (00:09-00:33, 11,286 lines), so nothing
  is lost to the 50 MB rotation.
- `sample-1-00-13.txt.gz`, `sample-2-00-17.txt.gz` - `sample 72874` of the wedged app, ~4 minutes apart. The pair is
  what proves the wedged threads are stuck rather than churning.
- `sample-dropbox-fileprovider.txt.gz` - `sample` of `DropboxFileProvider.appex` (pid 78509) during the wedge.

## Timeline

All times 2026-07-31, from `transfer-and-smb.log`.

- `00:10:57.987` copy admitted, `concurrency=8 (src=8, dst=10)`, `path=concurrent`.
- `00:10:58.039` the first 8 sources stream. All 8 land by ~`00:10:59.11` (~50 MB/s, healthy).
- `00:10:59.114-128` four more spawn: `sms-20260726002817.xml`, `calls-20260726002817.xml`, `sms-20260725002819.xml`,
  `calls-20260725002819.xml`.
- The two small `calls-*` take `write_from_stream`'s compound fast-path and complete.
- The two large `sms-*` log `stream: open_file_writer` and then emit nothing further, ever.
- `00:10:59.166` the driver logs its destination `get_metadata` pre-check for source 13 (`sms-20260724020237.xml`) and
  never logs the matching `spawning copy`.
- Silence. The remaining 752 sources were never started.
- `00:20:30` onward the log shows the app working normally (downloads watcher, a volume mount, the 00:25:42 updater
  check, global-shortcut refreshes). The backend was healthy; only the transfer subsystem was wedged.
- `00:33:09` force-quit. Shutdown itself was orderly (mDNS cleanup, `MCP server stopped`).

## Observed state during the wedge

Progress froze at 79.78 MiB and `5/764` files and never moved again across 20+ minutes of polling. Meanwhile the
destination actually held 10 complete files plus two incomplete ones:

```
sms-20260726002817.xml          0   <- zero bytes
sms-20260725002819.xml    4194304   <- exactly 4 MiB, truncated
```

Both sit at their **final names**, not `.cmdr-tmp-*`: a new-file copy has no conflict, so it takes no safe-replace temp.
After the force-quit these are indistinguishable from complete files by name.

## What was ruled out, and how

- **Not the source.** `dd` read both stuck source files directly during the wedge: 13 MB in ~3 ms from page cache. No
  file in the folder carries `SF_DATALESS`, so nothing was waiting on a Dropbox materialization.
- **Not the SMB connection.** `smb2::client::tree fs_info` kept succeeding every ~8 s throughout, and `tree: stat` (11x)
  and `tree: write_file_compound` (2x) ran _after_ the stall point.
- **Not a thread blocked in transfer code.** Both samples contain zero frames in `volume_copy`, `volume_strategy`,
  `checkpoint_stream`, or `smb2`. The driver and both stuck tasks are async-parked, invisible to a stack sample.
- **Not global runtime starvation.** ~14 `tokio::runtime::blocking::pool` threads were idle and awaiting work.
- **Not a frozen process.** The main thread is in its normal event loop, and unrelated subsystems kept logging.

## The separate, confirmed defect the samples caught

Both samples show 21-23 OS threads permanently blocked in `file_system::sync_status::get_ubiquitous_bool` ->
`NSURL getResourceValue:forKey:error:` -> `FPCFCopyAttributeValuesForItem` ->
`__NSXPCCONNECTION_IS_WAITING_FOR_A_SYNCHRONOUS_REPLY__`, 17 of them mid-XPC to `fileproviderd`. Still present, still
blocked, four minutes later.

`commands/sync_status.rs` wraps `get_sync_statuses` in a 2 s `blocking_with_timeout_flag`, but `spawn_blocking` work
cannot be cancelled: the timeout returns an empty map to the frontend while the `std::thread::scope` keeps holding a
Tokio blocking thread plus its ~11 spawned 8 MB-stack OS threads until the provider answers. The frontend then retries,
starting another batch; two rounds were in flight when sampled.

This folder is a worst case for it: with no dataless files, every one of the 764 paths misses the cheap `stat` shortcut
and takes the NSURL/XPC path.

Whether this contributed to the transfer wedge is **unknown** - different subsystems, no established link.

## Open questions the evidence cannot answer

1. What are the two `sms-*` copy tasks awaiting? A checkpoint park, an SMB response, or a lock?
2. Why did the driver stop after source 13's `get_metadata` pre-check without logging `spawning copy`, when six of its
   eight concurrency slots were free?
3. Did three SMB requests (two large writes plus one Create) genuinely go unanswered while small operations on the same
   connection kept flowing? Only `ChangeNotify` logs a `dispatch:` line, so Create and Write are invisible.
4. Why did Rollback do nothing? No `copy_volumes_with_progress: rolling back op=` line was ever written, so the intent
   never reached the driver or the driver never observed it while parked.

## Why the log could not answer them

- The transfer driver logs a task's spawn and its stream open, then nothing until completion. A task that stops
  mid-stream is indistinguishable from one that is merely slow.
- `checkpoint_stream`'s parks (user pause, source yield, destination yield) log nothing, so a park cannot be told from a
  hang.
- `smb2::client` logs request dispatch only on the `ChangeNotify` path; there is no outstanding-request accounting.
- `file_system::sync_status` has no logging at all, which is why 23 wedged threads left no trace in the log.

## Where the remediation lives now

Every gap above is closed, and every symptom this record describes has an owner. What replaced each, and why it took the
shape it did:

- **The four logging gaps**, answered by an in-flight phase table plus a stall watchdog rather than by more log lines:
  `apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "The stall signal". The `smb2` half
  (outstanding-request accounting for every command) finished as M0 of `docs/specs/smb-transfer-resilience.md`.
- **No byte-incomplete file at its final name**, the guarantee the two phone backups were owed: same file, § "File
  writes are staged", extended to local copies in § "Local copies stage".
- **Cancel and Rollback reaching a parked driver**: same file, § "Cancel and rollback reach a parked driver".
- **The stall notice that replaces a confident ETA**: `apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "The
  stalled-transfer notice". Why the frozen `5/764` counter was honest, and why surfacing the in-flight count was the
  answer rather than changing it:
  `apps/desktop/src-tauri/src/file_system/write_operations/transfer/transfer_driver/DETAILS.md` § "The file counter
  counts COMPLETED files".
- **The `sync_status` thread pile-up**, the separate defect the samples caught:
  `apps/desktop/src-tauri/src/file_system/sync_status/DETAILS.md`, with the before/after numbers in
  `docs/notes/sync-status-pool-bench-2026-07-31.md`.
- **The two windows disagreeing about the same byte count**: `apps/desktop/src/lib/units/DETAILS.md`, and
  `apps/desktop/src/lib/settings/DETAILS.md` § "Restricted-window mode" for the settings half of that split.

## Resolution (2026-08-01)

A second wedge, caught live, gave the answer this evidence could not.

**What was actually happening.** Two `ESTABLISHED` sockets, frozen for 40 minutes, with ~700 requests registered as
in-flight and **zero bytes moving in either direction** — while the log kept logging a fresh `fs_info` every 8 s, and a
brand-new `smb2` connection wrote 38 MB into the very same destination directory at 84 MiB/s. The NAS was never at
fault. `smb2`'s `TcpTransport::send` locked the transport's write half across its `write_all`; one send that never
finished parked every later request behind it, with no deadline, no error, and no log line.

**Why every guardrail missed it.** The 180 s response deadline and the 30 s credit deadline both live _downstream of the
send_, so neither could fire: the server had not been asked. `giving up` and `CreditStarvation` each appear **zero**
times in the whole log.

**Why it read as server silence for weeks.** `outstanding_requests()` timed each request from _registration_, which
happens before the bytes go out, under a doc comment reading "Requests sent and not yet answered". A never-sent request
was indistinguishable from an unanswered one. Three separate diagnoses blamed the server on that basis.

**The fix (smb2 0.15.0)**: a dedicated writer task owns the transport's write half and callers hand it whole frames;
each frame gets 60 s to reach the socket before `Error::SendTimeout` tears the connection down; waiters deregister on
drop; and `OutstandingRequest::sent_age` now says which side of the wire a request is on. Whole-frame handoff also
closed a second live hazard: a caller cancelled between the frame's length header and its body used to leave half a
frame on the wire, which Cmdr can trigger any time a user cancels a copy.

**Two footnotes worth keeping.** The `sync_status` XPC thread pile-up described above was a genuinely separate defect
and had no link to this wedge. And Samba 4.9.5 panics on repeated compound writes, written up in the `smb2` repo under
its own `docs/notes/` (that path is in THAT repo, not this one), which is unrelated to this incident but bites the same
code path on older servers.
