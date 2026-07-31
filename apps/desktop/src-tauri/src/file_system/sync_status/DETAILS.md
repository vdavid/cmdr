# Sync status: details

## Module map

- **`probe.rs`** — the OS question for one file. `stat`'s `SF_DATALESS` flag answers "is this a stub?" off the inode for
  free; an `NSURL` ubiquitous-item resource value answers "is it moving right now?" and is the expensive, unbounded
  part.
- **`pool.rs`** — a long-lived, hard-capped set of 8 MB-stack OS threads. Generic over boxed jobs; sync status is its
  only consumer today.
- **`cache.rs`** — answers keyed by directory then file name, with a two-tier TTL, LRU-by-directory eviction, and an
  injected clock so TTL behaviour is testable without sleeping.
- **`service.rs`** — cache lookup, batch join-or-supersede, cancellation, and the deadline. The public functions in
  `mod.rs` are one-line delegates to a `LazyLock<Service>`.
- **`bench.rs`** — the `#[ignore]`d before/after harness. See `docs/notes/sync-status-pool-bench-2026-07-31.md`.

## What went wrong, and what each part fixes

The 2026-07-31 transfer wedge (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`) was sampled twice, four
minutes apart. Both samples showed 21-23 OS threads permanently blocked in `get_ubiquitous_bool` → `NSURL
getResourceValue` → `__NSXPCCONNECTION_IS_WAITING_FOR_A_SYNCHRONOUS_REPLY__`, 17 of them mid-XPC to `fileproviderd`.

The old shape was: `commands/sync_status.rs` wrapped a `std::thread::scope` fan-out in a 2 s
`blocking_with_timeout_flag`. Four things compounded:

1. `spawn_blocking` work cannot be cancelled, so the 2 s timeout returned an empty map while the scope kept its tokio
   blocking thread and its ~11 spawned threads until the provider answered.
2. The threads were spawned **per call**, at `min(paths, available_parallelism())` a time, several times a second
   during scrolling.
3. Nothing was cached, so an unchanged folder paid the full price on every render and every 3 s idle poll.
4. The frontend retried a timed-out fetch, starting a second fan-out on top of the first. Two rounds were live when
   sampled.

Each is addressed by one piece: (1) the deadline now bounds only the wait, and the batch carries a cancellation flag;
(2) the pool; (3) the cache; (4) join-or-supersede.

## Decision: bound the leak, but replace lost workers

**Why:** a fixed-size pool is the obvious answer and it's wrong on its own. An XPC call into a provider that stops
answering never returns, so the worker is gone for the process's lifetime. With a plain fixed pool, one bad Dropbox
day would silently disable cloud badges until the user restarted Cmdr.

So `pool.rs` has both a `target_workers` (what it grows to lazily while every existing worker is busy) and a
`max_workers` (threads ever spawned, never exceeded). A worker on the same job for longer than `wedged_after` counts as
lost and may be replaced — within the ceiling. The leak is bounded by construction; a transient hang costs latency, not
the feature.

`max_workers` is 12, chosen to sit below the 21-23 the incident showed while leaving room to survive a handful of lost
workers. If all 12 are ever lost, batches stop resolving and the pane shows no badges: the honest outcome, and
`join_or_start` logs a warning naming the wedged count each time a batch starts.

## Decision: `target_workers` is 4, not `available_parallelism()`

**Why:** the work is XPC latency, not CPU, so worker count buys concurrency against a daemon rather than throughput.
Measured at ~18 ms per path per worker inside a Dropbox domain, four workers clear a 200-row visible range in ~900 ms
cold and instantly warm. The pane never asks for a whole folder — it asks for the range it just rendered. Sixteen
threads would halve a cold sweep the app doesn't perform, at four times the standing cost.

The cold-sweep trade is visible in the bench: 766 paths at once takes four workers past the 2 s deadline (443 answered)
where sixteen finished in 957 ms. That's accepted, because the remaining answers land in the cache behind the deadline
and the next poll picks them up for free.

## Decision: TTLs of 60 s stable and 2 s transitional

**Why:** `Uploading` and `Downloading` exist in order to become something else, so a badge stuck on them is a lie the
user can see; 2 s keeps them honest while still collapsing a render burst into one query. `Synced`, `OnlineOnly`, and
`Unknown` are settled, and every realistic way they change either produces an FSEvent (which invalidates through
`notify_directory_changed`) or is a user action that invalidates explicitly (`cloud_actions.rs`). So the stable tier can
be generous, and 60 s means a folder the user is staring at costs one round of provider calls a minute in the worst
case, none in the normal one.

`Unknown` sits in the stable tier deliberately: for the overwhelming majority of files (everything not in a cloud
folder) it is the permanent, correct answer, and it's what a whole ordinary directory returns.

## Decision: the cache is keyed by directory, not by full path

**Why:** invalidation arrives per directory and it arrives often — `notify_directory_changed` fires for every watcher
event, including throughout a big copy. A flat path map would make each of those an O(cache) scan. Keyed by directory,
it's one hash lookup whether it hits or misses. Eviction gets the same benefit: dropping a whole directory the user
navigated away from is one removal, and it's exactly the right granularity.

## Decision: the deadline lives in the module, not in `blocking_with_timeout_flag`

**Why:** the same reasoning as `commands/util.rs::timeout_detached`. An IPC deadline is a promise about the reply, not
permission to abandon the work. Applying it inside `statuses_within` lets the module return the paths it already knows
(cache hits plus whatever the batch resolved before the clock ran out) instead of the empty fallback the generic helper
would substitute, and lets the batch keep running so the answer is ready next time.

`commands/sync_status.rs` still owns the *value* of the deadline, so the "every FS-touching command is timed" contract
holds; it just hands it down rather than wrapping.

## Decision: don't build M4.5's cheap negative path

**Why:** measured, not assumed. A whole non-cloud directory (884 files in `/usr/bin`) costs ~19 ms wall and ~22 µs per
path, because `getResourceValue` short-circuits when no File Provider manages the URL — there's no XPC round-trip to
skip. Inside a domain, where a domain-root hint would say "yes, probe", the cost is ~4.5 ms per path. So the proposed
optimization saves microseconds on the cheap paths, nothing on the expensive ones, and adds an xattr read plus an
ancestor walk. Numbers: `docs/notes/sync-status-pool-bench-2026-07-31.md`.

If it's ever revived, the domain-root probe belongs in `cmdr-fs` as shared vocabulary. The canonical implementation is
`crates/cmdr-index/src/indexing/scanner/file_provider.rs`, which the app must not reach into (`index-crate-isolation`).

## Testing

- `pool.rs` tests pin the properties that matter with jobs that block on a channel, standing in for a provider that
  never replies: bounded threads across repeated bursts, never exceeding the ceiling when every job wedges, replacing a
  lost worker, and not fanning out for a single job.
- `service.rs` tests inject a counting `Probe`, so join, supersede, cancellation, the deadline, and the cache are all
  observable without a File Provider. `batches_started()` is the test-only hook that makes "joined rather than fanned
  out" an exact assertion instead of a timing guess.
- `cache.rs` tests step an injected clock rather than sleeping past a tiny TTL.
- Real XPC latency is only measurable against a real provider, which is what `bench.rs` is for.

## Follow-ups

- `pool.rs` is generic over boxed jobs and would suit `icons/mod.rs::fetch_path_icons` and `open_with.rs`, which both
  still spawn a per-call `std::thread::scope` of 8 MB threads for the same reason. Give each its own instance rather
  than sharing one: a wedged File Provider must not be able to stop icon fetching too.
