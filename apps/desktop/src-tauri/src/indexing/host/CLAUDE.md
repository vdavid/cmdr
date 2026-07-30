# The host seams

Everything the three index subsystems (`indexing/`, `media_index/`, `importance/`) need from the application around
them, as named seams instead of `crate::`-qualified reaches upward. This is the complete list, which is the point: the
index is being extracted into a Tauri-free crate, and this directory is what it will ask its host for.

## Must-knows

- **Add a seam here, never a new `crate::<app module>` import.** A back-edge from the three subsystems to any app module
  is what blocks the extraction, and it's checked, not trusted. If you need something from the app, it arrives through a
  trait or a config value declared here.
- **Each seam is injected once at startup and read through an accessor.** The accessors resolve to process-wide statics
  today; they become fields on the public `Index` handle later, without touching a single call site. So a call site
  should read the seam, never cache a handle in its own static.
- **❌ Nothing here lowers thread QoS, and the runtime you spawn onto has no bearing on it.** The heavy walking /
  writing / reconciling work runs on **dedicated** `std::thread`s that call `cmdr_fs::thread_qos` in their own bodies. A
  QoS class sticks to a thread for its whole life, so it can never be set on a pooled tokio worker. `DETAILS.md` §
  "The runtime seam and thread QoS".
- **❌ Never consult `HostPolicy` on a per-entry path.** It returns a cheap `Copy` value precisely so a caller takes ONE
  snapshot per batch (a listing top-up, a between-images gate, a resume poll) and reads it as often as it likes. Wanting
  a per-entry policy question means the call needs hoisting, not that the trait needs a method.
  `pace_tests::the_policy_is_consulted_per_listing_not_per_entry` pins it with a counting fake over a real scan.
- **A `WorkClearance` field is a decision, never a timestamp.** The elapsed-versus-threshold rule lives host-side, so
  there's one place that owns "how long counts as quiet". Read `volume_idle` when you contend for one share's
  connection (a network scan), `app_idle` when you contend for the machine (image enrichment). Mixing them up is how
  browsing a local folder ends up throttling a NAS scan.
- **`runtime::spawn` resolves a handle; `tokio::spawn` inherits one.** Indexing and the watcher can start from the app's
  synchronous `setup()` hook, where there's no ambient runtime, so `tokio::spawn` panics there. Spawn through the seam.

## Module map

- `runtime.rs` — the tokio runtime background work spawns onto (`set_runtime`, `spawn`, `spawn_blocking`, `block_on`).
- `policy.rs` — "may background work run right now?": `HostPolicy`, the `Copy` `WorkClearance` snapshot, `AlwaysClear`,
  and the test `FakeHostPolicy`. Implemented app-side by `priority::host_policy::AppHostPolicy`.

Rationale, the fallback runtime, and the QoS argument in full: `DETAILS.md`.
