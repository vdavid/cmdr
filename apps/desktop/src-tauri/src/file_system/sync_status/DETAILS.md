# Sync status: details

## Module map

- **`probe.rs`** — the OS question for one file, in three tiers. Is any ancestor a File Provider domain root
  (`cmdr_fs::file_provider`, an xattr read memoized per directory)? If not, done. Then `stat`'s `SF_DATALESS` flag
  answers "is this a stub?" off the inode; then an `NSURL` ubiquitous-item resource value answers "is it moving right
  now?", the expensive, unbounded part.
- **`pool.rs`** — a long-lived, hard-capped set of 8 MB-stack OS threads. Generic over boxed jobs; sync status is its
  only consumer today.
- **`cache.rs`** — answers keyed by directory then file name, with a four-tier TTL, LRU-by-directory eviction, and an
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

## Decision: four TTL tiers, because `Unknown` was two different facts

**Why:** the badge has five states but the cache has to reason about four LIFETIMES, and one of the five was carrying
two of them. `SyncStatus::Unknown` means "no badge" both for "no cloud provider owns this file" (permanent) and for "the
read didn't answer" (a failure that must be retried). They shared the 60 s stable tier, which made both wrong at once:
the permanent answer expired every minute, and a failed read would have been remembered as fact by any attempt to
lengthen it.

So the cache stores `SyncKnowledge`, not `SyncStatus`. It has six variants, `Unknown` is not one of them, and each maps
to exactly one tier:

- **`transitional`, 2 s** — `Uploading` / `Downloading`. These exist in order to become something else, so a badge stuck
  on them is a lie the user can see. Short enough to follow the transfer, long enough to collapse a render burst into
  one query.
- **`settled`, 60 s** — `Synced` / `OnlineOnly`. Every realistic way they change either produces an FSEvent (which
  invalidates through `notify_directory_changed`) or is a user action that invalidates explicitly (`cloud_actions.rs`),
  so a folder the user is staring at costs one round of provider calls a minute in the worst case and none in the
  normal one.
- **`structural`, 30 min** — `NotCloudManaged`. The permanent, correct answer for nearly every file on the machine. It
  changes only when a provider is installed (bounded by the domain resolver's own re-check, below) or when the file
  moves into a synced folder (which invalidates the directory). ❗ It stays BOUNDED rather than "forever until
  invalidated": nothing on the invalidation path fires when a user signs into iCloud, so an unbounded answer would be
  wrong until the next restart, and the bound costs one round of probes per folder per half hour.
- **`indeterminate`, 2 s** — `Indeterminate`. A read that didn't answer says nothing about the file, so it's cached only
  to damp a retry burst.

The type is what enforces this rather than a comment: `Ttls::for_knowledge` is a total match over `SyncKnowledge`, so a
new kind of answer cannot silently inherit somebody else's lifetime.

**What it was worth:** an idle app ran about 43 sync-status batches a minute learning "still not a cloud file", purely
because the negative expired every 60 s while `notify_directory_changed` was already invalidating on every real change.
⚠️ Sized honestly that is an IO-and-provider-load win; the probe's claim on CPU was refuted by measurement
(`docs/notes/idle-cpu-attribution-2026-08-03.md`, wrong answer 3: 3.4% of busy CPU but 0.2% of USERSPACE CPU, with
1,964 of 2,037 samples per thread inside the `stat` itself). Don't quote a CPU number for this anywhere.

## Decision: skip the provider entirely outside a File Provider domain

**Why:** the cheapest question is the structural one. `cmdr_fs::file_provider::FileProviderDomains` answers "is this
path, or any ancestor of it, a domain root?" from an xattr the provider daemon writes. The ancestor walk is memoized per
directory, so a row costs one xattr read on itself and a hash lookup for its folder, and the probe returns
`NotCloudManaged` without the `stat` and without the provider round-trip.

**Measured** (`bench.rs::bench_outside_a_domain`, 884 files in `/usr/bin`, release, macOS 26.6, 2026-08-21): **13.9 µs
per path against 63.9 µs** for the same probe forced down the full path. The leaf xattr read is essentially all of that
13.9 µs — the memoized directory verdict is 91 ns — so if this ever needs to be cheaper, that read is the thing to
sharpen (`xattr::get` sizes the value with one syscall and reads it with a second), ❌ not the walk.

**Why the leaf is read at all, rather than only the parent's ancestors:** a domain root is a row in its parent's
listing, and its parent is an ordinary folder. `~/Library/CloudStorage/Dropbox` answers
`NSURLUbiquitousItemIsUploadingKey = false` (verified 2026-08-21) — that is, it carries a real badge — while
`~/Library/CloudStorage` itself answers "not applicable". Deciding membership from the parent alone would drop the badge
from every provider's top-level row.

**This reverses an earlier "don't build it", and the difference is the memo.** M4.5 proposed the same check PER PATH,
and per path it doesn't pay: the walk costs about as much as the ~22 µs the `getResourceValue` short-circuit already
costs outside a domain (`docs/notes/sync-status-pool-bench-2026-07-31.md`). Per DIRECTORY it's one walk for the whole
folder, and — the part M4.5 didn't have — it is what makes the 30-minute `structural` tier a structural claim instead of
a guess.

**Why an ancestor walk rather than enumerating the domain roots once:** there is no way to enumerate them.
`getDomainsWithCompletionHandler` returns nothing to a non-extension-hosting app (measured), and the only other
enumeration is "look in the places domains usually live", which is the path-prefix heuristic the research note rejects:
it misses `~/Library/Mobile Documents` (iCloud Drive's domain root is that directory itself, not its
`com~apple~CloudDocs` child) and breaks for any provider that registers elsewhere. The walk finds a domain wherever it
actually is, and the memo makes its cost a rounding error.

**Two bounds keep it honest**, both in `cmdr-fs`:

- Every verdict expires after 10 minutes, so installing Dropbox or signing into iCloud is noticed without a restart.
- The marker is a private, undocumented Apple xattr, so the resolver checks it against the domains THIS machine has
  before believing a negative. A machine with provider folders that carry no marker gets `Undetermined` and no fast path
  at all — if Apple ever drops the xattr, badges keep working and only the shortcut is lost.

**Accepted edge:** a symlink pointing at a file inside a domain, from outside one, reads as not cloud-managed. The walk
canonicalizes the DIRECTORY (which is what makes iCloud's "Desktop & Documents Folders" work, where `~/Desktop` is a
link into the domain) but not the leaf, because leaves are files and there are millions of them.

## Decision: pass `isDirectory:` when building the `NSURL`

**Why:** `NSURL(fileURLWithPath:)` stats the path to decide directory-ness, and the probe has just stat'ed it. Measured
on macOS 26.6 (2026-08-21, 200,000 iterations per variant, `target/scratch/urlbench.swift` shape): building the URL
costs 4.08 µs without `isDirectory:` and 0.53 µs with it, against 3.16 µs for a bare `stat()` on the same path; the
gap survives all the way through `getResourceValue` (35.4 µs → 30.4 µs), so the syscall is **removed, not deferred** to
the first resource-value read. ⚠️ The value must come from that same `metadata()` call, never a guess: `isDirectory:`
decides whether the URL keeps a trailing slash, which changes the path the File Provider machinery matches on.

## Decision: the cache is keyed by directory, not by full path

**Why:** invalidation arrives per directory and it arrives often — `notify_directory_changed` fires for every watcher
event, including throughout a big copy. A flat path map would make each of those an O(cache) scan. Keyed by directory,
it's one hash lookup whether it hits or misses. Eviction gets the same benefit: dropping a whole directory the user
navigated away from is one removal, and it's exactly the right granularity.

### Known limit: two slow providers at once

The single in-flight batch is global, not per pane. Two panes showing *different* cloud folders alternate their 3 s
polls, so each supersedes the other and only the paths a worker already picked up get cached per round. It converges
(the cache fills monotonically, and a pane stops creating batches once its visible range is cached) but slowly.

In practice it barely bites, because it needs **both** folders to be slow: a pane on an ordinary or network folder
resolves its whole visible range in milliseconds (~22 µs per path, see the bench note), so it supersedes the cloud pane
for an instant rather than starving it. The incident's own shape — Dropbox on one side, an SMB share on the other — is
the benign case.

If it ever does bite, the fix is one line rather than a redesign: hold a small queue of in-flight batches instead of
one, cancelling the oldest beyond the cap. Capping at one was about wasted provider calls; the thread count it also used
to protect is now the pool's job, so the cap can rise without giving anything back.

## Decision: the deadline lives in the module, not in `blocking_with_timeout_flag`

**Why:** the same reasoning as `commands/util.rs::timeout_detached`. An IPC deadline is a promise about the reply, not
permission to abandon the work. Applying it inside `statuses_within` lets the module return the paths it already knows
(cache hits plus whatever the batch resolved before the clock ran out) instead of the empty fallback the generic helper
would substitute, and lets the batch keep running so the answer is ready next time.

`commands/sync_status.rs` still owns the *value* of the deadline, so the "every FS-touching command is timed" contract
holds; it just hands it down rather than wrapping.

## Testing

- `pool.rs` tests pin the properties that matter with jobs that block on a channel, standing in for a provider that
  never replies: bounded threads across repeated bursts, never exceeding the ceiling when every job wedges, replacing a
  lost worker, and not fanning out for a single job.
- `service.rs` tests inject a counting `Probe`, so join, supersede, cancellation, the deadline, and the cache are all
  observable without a File Provider. `batches_started()` is the test-only hook that makes "joined rather than fanned
  out" an exact assertion instead of a timing guess.
- `cache.rs` tests step an injected clock rather than sleeping past a tiny TTL, and `service.rs` borrows the same clock
  through `Service::with_clock` so "twenty minutes of idle polling cost zero provider calls" is an exact assertion.
  ❗ The pair that IS the change: `a_not_a_cloud_file_answer_outlasts_the_idle_poll_by_far` (expiry no longer drives the
  re-probe) and `invalidation_forces_a_re_probe` (invalidation still does, on the longest-lived tier there is). The
  whole design is trusting invalidation over expiry, so neither test means much without the other.
- `probe.rs` tests inject the domain resolver (`FileProviderDomains::with_domain_roots`) rather than depending on which
  cloud apps the machine running them has. A path that doesn't exist is the lever: outside a domain the shortcut answers
  `NotCloudManaged` without a syscall, inside one the `stat` runs and fails, so the two answers prove the shortcut
  really skipped the filesystem.
- Real XPC latency is only measurable against a real provider, which is what `bench.rs` is for.

## Follow-ups

- `pool.rs` is generic over boxed jobs and would suit `icons/mod.rs::fetch_path_icons` and `open_with.rs`, which both
  still spawn a per-call `std::thread::scope` of 8 MB threads for the same reason. Give each its own instance rather
  than sharing one: a wedged File Provider must not be able to stop icon fetching too.
