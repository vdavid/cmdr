# Host seam details

## Why the seams exist at all

`indexing/`, `media_index/`, and `importance/` are being lifted out of the app into a workspace crate with no `tauri` in
its dependency tree. Every remaining reference from those trees to an app module is a back-edge that blocks the move, so
each one gets a disposition: move the thing down into `cmdr-fs`, inline it, or turn it into a seam here.

A seam is worth it when the answer genuinely belongs to the product rather than to the index: which runtime to run on,
which volumes exist, whether now is a good time to do background work, what the user's settings say. Those are host
decisions; the index only needs to ask.

## The shape every seam takes

Injected once at startup, read through a `pub(crate)` accessor that resolves a process-wide static.

That static is deliberate, not an oversight. The subsystems carry roughly 50 process-wide statics today and
de-globalizing them is a lifecycle rewrite with real bug risk. Splitting the two problems means call sites get written
against the good shape immediately (`ask the seam`), so when the statics later move into the `Index` handle, that's a
pure internal change with no call-site churn.

The corollary for anyone adding a call site: **read the seam where you need it, don't stash the result in a static of
your own.** A cached copy is what would have to be chased down later.

## The runtime seam and thread QoS

`tauri::async_runtime::spawn` **is** `tokio::spawn` on Tauri's runtime, so replacing ~66 of those calls with a handle
the app injects is behavior-preserving by construction. What made it worth checking rather than assuming is the QoS
question, since thread scheduling class is the property that lets indexing run inside the app process at all: a runaway
scan must never outrank the webview for CPU.

**The runtime a task is spawned onto has no bearing on QoS.** The seven `set_current_thread_qos` call sites all sit at
the top of a **dedicated** `std::thread::Builder::spawn` body — `scanner/mod.rs`, `scanner/walker/mod.rs` (worker and
watchdog), `writer/mod.rs`, `reconcile/local_reconcile.rs` (reader and walk), and `reconcile/reconciler/rescan/mod.rs`.
Those threads are created by index code and are not tokio's to schedule; a tokio task that starts one is just the
caller. macOS QoS is per-thread and set explicitly here, so nothing is inherited from whoever spawned the task either.

**Which is also why no seam may set QoS.** `runtime::spawn_blocking` hands work to a pooled thread, and a class set on a
pooled thread persists for that thread's whole life, leaking a lowered priority onto unrelated tasks that land there
next. `rescan.rs` documents choosing a dedicated thread over the blocking pool for exactly this reason.

The structural argument is checkable but not self-enforcing, so it was also verified against the running app: method and
per-thread numbers in `docs/notes/index-extraction-baseline.md` § "Thread QoS after the runtime swap". Redo that
measurement if the spawn topology ever changes; `set_current_thread_qos` is a no-op in test builds, so no unit test can
catch a regression here.

### The fallback runtime

When nothing has been injected, the first spawn lazily builds one multi-threaded runtime and keeps using it. That isn't
a second competing pool in any shipped configuration — the app injects at the top of `setup()`, before anything can
start background work — it's what keeps test binaries, benches, and the `index-query` tools working, and it mirrors
exactly what `tauri::async_runtime` does for a host that never calls `set`.

A second `set_runtime` loses and reports `RuntimeAlreadySet` rather than panicking or swapping the runtime under tasks
that are already running on the first one. Stranding live tasks to honor a late caller would be the worse failure.

## The host policy seam

Two consumers, one question. A network index scan asks at every listing top-up (`network_scanner/scan_pace.rs`) and a
media-enrichment pass asks between images and while parked waiting to resume (`media_index/scheduler/`). Both stand
aside the same way and both used to reach `crate::priority` directly.

**Why one trait rather than two.** The priority ORDER — user-interactive work, then transfers, then indexing — is a
single product decision. Splitting it across a foreground trait and a transfer trait would let the two drift, and
neither consumer ever wants one without the other.

**Why the scopes are both in the snapshot.** `app_idle` and `volume_idle` answer different questions and are not
interchangeable: enrichment is heavy on-device ML competing for the whole machine, so any browsing is reason to wait,
while a network scan contends for one share's SMB session, so browsing a local folder must not slow it. Computing both
costs nothing (one atomic load and one small map read) and it means a consumer picks its scope at the point where the
reasoning is written down, rather than by choosing which function to call.

### Why priority roots are a method here, and per volume

Two things the index can't work out for itself are which folders matter to this user and where the user is looking right
now. The second one needed no new door: `open_listings` already reports every directory a pane is showing. The first is
`priority_roots(volume_id)`, and it lands on this trait rather than as an argument to a launch call for one reason: an
answer frozen at launch goes stale the moment somebody edits their favorites or opens a tab, and re-plumbing it would
mean a new call path per signal. Asked on demand, the host can answer from whatever it knows at that moment.

**It is an order, and the index may conclude nothing else from it.** The paths carry no scope and no promise: the walk
covers the whole volume either way, so a root that appears or disappears between two asks changes what the user gets
first and nothing about what they eventually get. That is what makes a guess safe to act on.

**Per volume, because every signal behind it is about one machine's layout.** A share must not inherit the boot drive's
home folder, and a favorite pointing into a share is a folder on a different index. Passing the volume lets the host
answer where it can and stay quiet where it can't, without the index having to route paths back to volumes.

**The cost rule is `clearance`'s, one level relaxed.** It allocates, like `open_listings`, and a real host has to stat
things to know a folder is there, so the contract is "cheap per ask, cached behind a short TTL host-side" rather than
"free". The app's answer lives in `apps/desktop/src-tauri/src/priority/roots.rs`.

### Why the FDA gate isn't a method here

The plan expected `fda_gate::is_fda_pending` to need a `HostPolicy` method, on the grounds that it's a runtime query. It
isn't: the index's only reference is to the **pure two-argument** decision, from inside `should_auto_start_indexing`,
which is itself a pure function over caller-supplied values. (The process-global `is_fda_pending_runtime` exists, but no
index code calls it.)

So the disposition is cheaper than a seam: `should_auto_start_indexing` now takes `fda_pending: bool`, the app resolves
the rule where the FDA choice and the OS probe already live, and the back-edge is gone with no trait at all. The choice
× OS-granted truth table stays tested once, in `fda_gate`.

### The write half

A query-only trait returning a `Copy` value gives a test nothing to manipulate, and the real signals live in
process-global maps a test can nudge but never reset. So `FakeHostPolicy` carries setters — `note_foreground_activity`,
`note_foreground_quiet`, `note_transfer_started`, `note_transfer_finished`, `note_open_listing`, `note_priority_root` —
plus a call counter, and `ScanPacer::with_policy` takes one directly. The pacing tests no longer touch a global at all,
which is also what makes them safe to run in parallel.

The counter is not incidental: it's the evidence for the per-batch dispatch rule. With four directories of 250 files, a
compliant walk asks 15 times; adding a single `clearance()` call to the per-entry loop takes it to 1,015 and the guard
fails.

## The volume seam

`VolumeProvider` covers five things the app is the only one that can answer: what's registered right now, what
filesystem a path sits on, whether a volume's filesystem is still mounted, how to turn an OS-mounted share into a direct
smb2 session, and what a PTP object handle resolves to. All at human cadence — once per scan start, per watch event, per
enrichment pass.

**What deliberately isn't on it.** Volume ID vocabulary: `mtp_ids` was nine references to pure string work with no host
behind it, so it moved to `cmdr-fs` beside `smb_volume_id` rather than becoming four trait methods. The test is whether
you could compute the answer from a `&str`.

**`MountFacts` is two decisions, not a `FilesystemKind`.** The index acts on exactly two things — may the local walker
touch this mount, and may the rename pre-pass trust its inodes — and both are host judgments: the mount probe is
per-platform (the app answers "network" for anything that isn't a known local disk), and the probe itself can block for
minutes on a wedged mount. Returning the two flags moved the whole macOS/Linux fork out of `transports/local_external`.

**Presence is by filesystem identity, and it reads the mount table, never the mount.** `mount_identity(root)` names the
filesystem mounted exactly at a root (a `MountIdentity`, opaque to the index), and `is_mounted(identity)` says whether
any mounted filesystem has it, `None` when the table couldn't be read. Callers name that third answer rather than
folding it: `VolumeHold::drive_is_listed` is `== Some(true)`, so an unreadable table reads as NOT listed, which is the
safe direction for every delete and completion gate hanging off it (`../hold.rs`). ❌ Never by root path: renaming a
mounted volume moves its mount point while the filesystem stays mounted under it and an open handle keeps writing
(verified on macOS 26.6.2, APFS and HFS+ images, `cmdr_fs::testing::disk_images::real_images` and the app's
`file_system::index_provider::real_image`, 2026-09-14), so a missing root is not a drive that's gone. The app reads
`f_fsid` from `getfsstat(MNT_NOWAIT)` on macOS (`volumes::mounts::{mount_identity_at, has_mount_identity}`) and
`major:minor` from `/proc/self/mountinfo` on Linux (`file_system::linux_mounts::{mount_device_at, device_is_mounted}`),
so a dead drive can't stall either. A local-scanner reservation reads the identity just before its lock and the hold
generation keeps it (`../hold.rs`). `NoVolumes` names no filesystem (and reads every identity as mounted: nothing can
unmount under it), so a start without a host captures nothing to ask about. `FakeVolumeProvider` mounts, renames, and
unmounts roots with `mount`, `rename_mount`, and `mark_unmounted`. `stop_removable_volume` asks
(`../lifecycle/DETAILS.md` § "When a volume has been let go"), and so does every delete and completion gate, through
`VolumeHold::drive_is_listed` (`../reconcile/DETAILS.md` § "The delete gates").

**`mount_points()` is the whole table, as it spells itself**, from the same non-blocking read (`volumes::mount_roots` /
`volumes_linux::mount_roots` app-side), `None` when it couldn't be read. The index filters it into the mounts inside the
boot tree and caches that for a second (`../scanner/boot_tree_mounts.rs`). `FakeVolumeProvider::mount` lists a root
there too, so a test that mounts a fake drive at a temp path inside the boot tree has made it a boot-tree mount, which
boot-space walks then stop at; list a gate-only drive under `/Volumes/` instead (`reconciler/tests/delete_gates.rs`).
Installing a provider, and every fake table change, bumps `table_generation`, so the cache re-reads at once.

**Why the provider slot is an `RwLock`, unlike the runtime and the policy.** Tests swap it. Three tests used to register
real `LocalPosixVolume`s into the process-wide `VolumeManager`, which is exactly the coupling the extraction removes;
they now build a `FakeVolumeProvider` and install it under a guard that restores on drop. The slot is still
process-wide, so `test_lock()` serializes them for a plain `cargo test` (nextest's process-per-test wouldn't need it).

## The config seam

`IndexConfig` is an INPUT value, not a stored snapshot. `set_config` pushes the media half straight into the gate
atomics and the network-enrichment config, and keeps only the data dir.

**That asymmetry is the design, not an oversight.** The media-policy IPC setters live-apply single fields as the user
moves a slider, so a stored copy of `enabled` or `scope` here would go stale the moment one ran, and "what is the index
configured to do" would have two answers. The gate atomics stay the one place those values live; the data dir has no
other home, so it lives here.

`commands/media_index::index_config_from` is where settings become policy — every default, every migration fallback (the
pre-setting scope inference), every clamp. Nothing inside the index reads a settings file.

## The event seam

`events.rs` is only an injection point; the trait and the `IndexEvent` enum belong to `../events/`. It exists because
two places START a subsystem — a drive index and the media scheduler — and both used to build an app-side
`TauriEventSink` inline, which put the app's event mapping back inside the index. Everything downstream already threads
`Arc<dyn EventSink>` through constructors, which is the shape to keep: read the seam where a subsystem starts, then pass
it down.

## Cancellation

One primitive, `tokio_util::sync::CancellationToken`, from the `Volume` trait in `cmdr-fs` up through every long walk
`indexing/`, `media_index/`, and `importance/` run. It replaced five kinds of `Arc<AtomicBool>` plus a `Notify`, none of
which could compose.

**The topology is a tree, rooted per volume.** The reservation mints the volume's root `VolumeWork` (`../hold.rs`: a
stop signal paired with a share of the volume's hold), held by BOTH the registry `IndexInstance` (`work`) and its
`IndexManager`, so the two can't disagree. Everything below it runs on a child, and a child that reads the drive is a
`VolumeWork` too, so a removable stop can wait for it:

- the full scan and the network trait scan (`manager::start_scan`, `network_scan::start_volume_scan`)
- the local reconcile walk
- the subtree-rescan drain, the per-navigation verifier, and background verification — the three that used to carry a
  flag hardcoded to `false`, i.e. couldn't be stopped at all
- inside a scan, the walker's own token, which the insert visitor cancels on a writer-send failure

That last one is why the child relationship matters beyond tidiness: cancelling the CHILD stops the walk without the
scan reading as user-cancelled, which is what `run_scan` asks the PARENT for. It also deleted a dedicated bridge thread
that existed only to mirror one flag into another.

`stop_scan` cancels one scan's child, so the volume can start another; `shutdown` cancels the volume token, so
everything under it stops at once.

**A child is handed DOWN to the work, never looked up by volume id.** The manager passes one into `ScanCompletion`,
`ReplayConfig`, and the phase machine (and from there into `EventReconciler` and background verification);
`trigger_verification` passes one into `maybe_verify` while it still holds the instance; `cover_context_for` mints a
cover walk's under the same registry lock that hands out its writer. This is both what keeps `lifecycle::state` out of
the layers below and what makes cancellation correct: a walk that resolved its token after its volume was torn down
would find nothing, default to a token that never fires, and run on into a draining writer. A test fixture with no
volume behind it takes `VolumeWork::for_test`, a generation of its own that nothing else stops.

**`media_index` shares the primitive but not the tree.** Its emergency stop (`gate::stop_token`) is process-wide, and
re-enabling installs a FRESH token rather than un-cancelling — a token is one-shot by design, and a pass the user
stopped must not quietly resume. Per-volume media cancellation would be a new feature, not a rewiring: nothing today
scopes an enrichment pass to a volume's token. Its per-volume LISTENERS are in the tree, though: `wire_volume` takes the
same child token importance does and ends them with the volume (`media_index/scheduler/DETAILS.md` § The lifecycle bus).

**`importance` is in the tree, holding a bare token.** A volume reaches the importance scheduler with a child of its
root token already attached (`lifecycle_bus::RegisteredVolume.stop`, minted at the registration funnel and, for the
startup sweep, under the registry lock in `state::ready_volumes_to_wire`), so it is handed down like every other child.
It is deliberately NOT a `VolumeWork`: a pass reads the local index database and never the drive, so it must not hold
the volume against an eject. The scheduler also registers a `register_subsystem_stop_hook`, so `stop_all_indexing`
reaches every pass at once. How a pass polls, and what a stopped one leaves behind: `importance/scheduler/DETAILS.md` §
"How a pass stops".

## Cancellation is observable, as a typed error

`Ok` from a scan means the walk FINISHED. A stopped walk returns `ScanError::Cancelled(ScanSummary)` /
`VolumeScanError::Cancelled(ScanSummary)`, carrying its partial totals.

**Why a variant and not a `was_cancelled` flag on the summary:** `scan_completed_at` is written off that distinction,
and writing it for a partial strands the index permanently — the next launch skips the healing rescan forever. A flag on
a success value lets a caller reach the marker-writing code by simply not checking; an error variant means there is no
`Ok`-shaped partial to hold. ❌ Don't "simplify" it back.

**Cancelled is a third outcome, not a second failure.** `run_scan_completion` splits once, into
`(summary, was_completed)`: a cancelled walk takes the same post-scan handoff as a clean one (its rows are real and want
reconciling) but writes no meta and touches no freshness, while genuine failures route to `report_unfinished_scan` and
get `ScanFailed` ⇒ Stale. On the network side `Cancelled` is deliberately NOT a terminal disconnect, so the partial is
discarded rather than kept as Stale. Collapsing cancelled into either neighbour is the bug to hunt for.

**The split is public, not internal.** `run_scan` keeps a `ScanOutcome` because the write sequences that follow a walk
need its ids and epoch on the cancel path too: `scan_subtree` already sent the destructive `DeleteDescendantsById`, so
it stamps and aggregates FIRST and only then unwraps to a `Result`.
`a_cancelled_subtree_scan_still_repairs_its_ancestors` pins that ordering.
