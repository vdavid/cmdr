# Sync-status rework: before and after (2026-07-31)

The M4 numbers for `apps/desktop/src-tauri/src/file_system/sync_status/`, taken on the folder from the transfer-wedge
incident. Keep this until the next time someone asks "how many threads should the badge cost?"; the per-path latencies
below are the input to that answer.

## Method

`sync_status::bench` (`bench.rs`, `#[ignore]`d) runs both shapes back to back in one process against a directory named
by `CMDR_SYNC_STATUS_BENCH_DIR`, so they see the same provider, the same page cache, and the same machine:

```sh
CMDR_SYNC_STATUS_BENCH_DIR="$HOME/Library/CloudStorage/Dropbox/Apps/SMSBackupRestore" \
  cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --release --lib \
  file_system::sync_status::bench -- --ignored --nocapture --test-threads=1
```

- **Before** is the pre-M4 shape, reproduced inside the bench: a `std::thread::scope` fan-out of
  `min(paths, available_parallelism())` fresh 8 MB-stack threads per call, no cache, nothing shared between calls.
- **After** is the shipped service: a hard-capped long-lived pool, one batch in flight, per-directory TTL cache.
- Threads are counted at the spawn site; CPU is a `getrusage(RUSAGE_SELF)` delta (user + system) across the whole run;
  live process threads come from `proc_pidinfo(PROC_PIDTASKINFO)`.

Environment: macOS 26.5.2, Apple silicon, release build, 2026-07-31. Source folder
`~/Library/CloudStorage/Dropbox/Apps/SMSBackupRestore`, 766 files, Dropbox File Provider domain, none dataless (so every
path misses the `stat` shortcut and takes the XPC path: the worst case, and the incident's case).

Both shapes run in the same process, **before first**. That biases the comparison _towards_ the old shape, since the new
one meets a provider the old one just warmed up. The wins below are therefore conservative.

## Steady pane: 100 visible rows, 20 rounds

The real-world case. It's what a pane does over a minute of a user looking at one cloud folder: every listing render
plus the 3 s idle poll re-asks for the visible range.

|                  | before | after  |
| ---------------- | ------ | ------ |
| threads spawned  | 300    | 0      |
| wall time        | 3 s    | 455 µs |
| CPU (user + sys) | 996 ms | 454 µs |
| paths answered   | 100    | 100    |

Round 1 fills the cache; rounds 2-20 never reach the provider. Zero threads spawned because the pool's four workers
already existed. This is the "resource efficiency" line: a minute of sitting still on a Dropbox folder went from a
second of CPU and 300 thread creations to under half a millisecond and none.

## Cold sweep: all 766 paths at once

A stress case rather than a real one (the pane asks about its visible range, not a whole folder), included because it's
where the bounded pool costs something instead of saving something.

|                  | before | after                  |
| ---------------- | ------ | ---------------------- |
| threads spawned  | 16     | 1                      |
| wall time        | 957 ms | 2 s (hit the deadline) |
| CPU (user + sys) | 377 ms | 238 ms                 |
| paths answered   | 766    | 443                    |

Sixteen threads answer 766 paths faster than four do. That's the deliberate trade: the four are permanent and capped,
the sixteen were per call. The 323 unanswered paths are **not lost** — the batch keeps running past the deadline and its
answers land in the cache, so the pane's next poll has them without touching the provider. The frontend already retries
a timed-out fetch.

Derived: **~18 ms per path per worker** inside this domain. That is the number to size the pool with. At four workers a
200-row visible range costs ~900 ms cold and nothing warm, which is why `target_workers` is 4 and not 16.

## The cheap-negative question (M4.5)

M4.5 proposed skipping the NSURL query outside a known File Provider domain root. The premise doesn't survive
measurement. Same bench, same build, pointed at `/usr/bin` (884 files, no provider anywhere):

|                  | before | after |
| ---------------- | ------ | ----- |
| threads spawned  | 16     | 3     |
| wall time        | 20 ms  | 19 ms |
| CPU (user + sys) | 127 ms | 56 ms |
| paths answered   | 884    | 884   |

**~22 µs per path outside a domain, versus ~4.5 ms inside one** (wall, whole-folder, per path). `getResourceValue`
short-circuits when no File Provider manages the URL; there is no XPC round-trip to skip. So a domain-root pre-check
would save ~22 µs on paths that are already nearly free, save nothing on the paths that actually hurt (inside a domain
the hint says "yes, probe"), and add an xattr read plus an ancestor walk of its own. The TTL cache absorbs the 22 µs
anyway, once per directory.

Conclusion, as of this bench: don't build it PER PATH, which is what was proposed.

**Superseded on 2026-08-21, and the measurement above is why it changed shape rather than being ignored.** The check now
runs per DIRECTORY, memoized (`cmdr_fs::file_provider::FileProviderDomains`), so the ~22 µs it saves is multiplied by
the whole visible range while the walk is paid once. The bigger half is that a structural negative is a permanent fact
where a probe's negative wasn't, which is what let the cached "not a cloud file" answer go from 60 seconds to 30
minutes and take an idle app's 43 sync-status batches a minute with it. The reasoning lives with the code:
`apps/desktop/src-tauri/src/file_system/sync_status/DETAILS.md`. As predicted here, the probe moved to `cmdr-fs` as
shared vocabulary rather than being duplicated app-side.
