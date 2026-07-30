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
