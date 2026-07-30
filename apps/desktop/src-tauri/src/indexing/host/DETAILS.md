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
watchdog), `writer/mod.rs`, `reconcile/local_reconcile.rs` (reader and walk), and `reconcile/reconciler/rescan.rs`.
Those threads are created by index code and are not tokio's to schedule; a tokio task that starts one is just the
caller. macOS QoS is per-thread and set explicitly here, so nothing is inherited from whoever spawned the task either.

**Which is also why no seam may set QoS.** `runtime::spawn_blocking` hands work to a pooled thread, and a class set on
a pooled thread persists for that thread's whole life, leaking a lowered priority onto unrelated tasks that land there
next. `rescan.rs` documents choosing a dedicated thread over the blocking pool for exactly this reason.

The structural argument is checkable but not self-enforcing, so it was also verified against the running app: method
and per-thread numbers in `docs/notes/index-extraction-baseline.md` § "Thread QoS after the runtime swap". Redo that
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

Two consumers, one question. A network index scan asks at every listing top-up
(`network_scanner/scan_pace.rs`) and a media-enrichment pass asks between images and while parked waiting to resume
(`media_index/scheduler/`). Both stand aside the same way and both used to reach `crate::priority` directly.

**Why one trait rather than two.** The priority ORDER — user-interactive work, then transfers, then indexing — is a
single product decision. Splitting it across a foreground trait and a transfer trait would let the two drift, and
neither consumer ever wants one without the other.

**Why the scopes are both in the snapshot.** `app_idle` and `volume_idle` answer different questions and are not
interchangeable: enrichment is heavy on-device ML competing for the whole machine, so any browsing is reason to wait,
while a network scan contends for one share's SMB session, so browsing a local folder must not slow it. Computing both
costs nothing (one atomic load and one small map read) and it means a consumer picks its scope at the point where the
reasoning is written down, rather than by choosing which function to call.

### Why the FDA gate isn't a method here

The plan expected `fda_gate::is_fda_pending` to need a `HostPolicy` method, on the grounds that it's a runtime query.
It isn't: the index's only reference is to the **pure two-argument** decision, from inside
`should_auto_start_indexing`, which is itself a pure function over caller-supplied values. (The process-global
`is_fda_pending_runtime` exists, but no index code calls it.)

So the disposition is cheaper than a seam: `should_auto_start_indexing` now takes `fda_pending: bool`, the app resolves
the rule where the FDA choice and the OS probe already live, and the back-edge is gone with no trait at all. The
choice × OS-granted truth table stays tested once, in `fda_gate`.

### The write half

A query-only trait returning a `Copy` value gives a test nothing to manipulate, and the real signals live in
process-global maps a test can nudge but never reset. So `FakeHostPolicy` carries setters —
`note_foreground_activity`, `note_foreground_quiet`, `note_transfer_started`, `note_transfer_finished` — plus a call
counter, and `ScanPacer::with_policy` takes one directly. The pacing tests no longer touch a global at all, which is
also what makes them safe to run in parallel.

The counter is not incidental: it's the evidence for the per-batch dispatch rule. With four directories of 250 files,
a compliant walk asks 15 times; adding a single `clearance()` call to the per-entry loop takes it to 1,015 and the
guard fails.

## The volume seam

`VolumeProvider` covers four things the app is the only one that can answer: what's registered right now, what
filesystem a path sits on, how to turn an OS-mounted share into a direct smb2 session, and what a PTP object handle
resolves to. All at human cadence — once per scan start, per watch event, per enrichment pass.

**What deliberately isn't on it.** Volume ID vocabulary: `mtp_ids` was nine references to pure string work with no host
behind it, so it moved to `cmdr-fs` beside `smb_volume_id` rather than becoming four trait methods. The test is whether
you could compute the answer from a `&str`.

**`MountFacts` is two decisions, not a `FilesystemKind`.** The index acts on exactly two things — may the local walker
touch this mount, and may the rename pre-pass trust its inodes — and both are host judgments: the kind → network
mapping is per-platform, and the probe itself can block for minutes on a wedged mount. Returning the two flags moved
the whole macOS/Linux fork out of `transports/local_external`.

**Why the provider slot is an `RwLock`, unlike the runtime and the policy.** Tests swap it. Three tests used to
register real `LocalPosixVolume`s into the process-wide `VolumeManager`, which is exactly the coupling the extraction
removes; they now build a `FakeVolumeProvider` and install it under a guard that restores on drop. The slot is still
process-wide, so `test_lock()` serializes them for a plain `cargo test` (nextest's process-per-test wouldn't need it).

## The config seam

`IndexConfig` is an INPUT value, not a stored snapshot. `set_config` pushes the media half straight into the gate
atomics and the network-enrichment config, and keeps only the data dir.

**That asymmetry is the design, not an oversight.** The media-policy IPC setters live-apply single fields as the user
moves a slider, so a stored copy of `enabled` or `scope` here would go stale the moment one ran, and "what is the index
configured to do" would have two answers. The gate atomics stay the one place those values live; the data dir has no
other home, so it lives here.

`commands/media_index::index_config_from` is where settings become policy — every default, every migration fallback
(the pre-setting scope inference), every clamp. Nothing inside the index reads a settings file.

## The event seam

`events.rs` is only an injection point; the trait and the `IndexEvent` enum belong to `../events/`. It exists because
two places START a subsystem — a drive index and the media scheduler — and both used to build an app-side
`TauriEventSink` inline, which put the app's event mapping back inside the index. Everything downstream already threads
`Arc<dyn EventSink>` through constructors, which is the shape to keep: read the seam where a subsystem starts, then
pass it down.

## Cancellation is still `Arc<AtomicBool>`

The plan wants one cancellation primitive, `tokio_util::sync::CancellationToken`, replacing the `Arc<AtomicBool>` flags.
That isn't done, and it isn't index-internal: `Volume::list_directory_for_scan` and `list_directory_with_cancel` carry
`Option<&Arc<AtomicBool>>` in **`cmdr-fs`**, implemented by every real backend (SMB in two files, MTP in two). So the
swap is a `cmdr-fs` trait change plus every implementor plus ~100 call sites, and a partial swap would leave two
primitives, which is worse than one.

Also unresolved with it: whether a cancelled scan should return a distinct error variant instead of
`ScanSummary { was_cancelled: true }`. Cancellation IS observable today — it's a returned value, not a silent early
return — but the variant would fork the completion handler's arms, and that handler decides whether `scan_completed_at`
gets written. A false "complete" permanently strands an index, so this is not a change to make in passing.
