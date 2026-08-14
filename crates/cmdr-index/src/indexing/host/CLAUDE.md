# The host seams

Everything the three index subsystems (`indexing/`, `media_index/`, `importance/`) need from the app around them, as
named seams instead of `crate::`-qualified reaches upward. The completeness is the point: the index is being extracted
into a Tauri-free crate, and this directory is what it will ask its host for.

## Must-knows

- **Add a seam here, never a new `crate::<app module>` import.** A back-edge from the three subsystems to any app module
  is what blocks the extraction, and it's checked, not trusted. Anything you need from the app arrives through a trait
  or a config value declared here.
- **`Index::builder()` installs all five; an accessor reads one.** The accessors resolve process-wide slots today and
  become handle fields later, with no call-site churn, so read the seam where you need it, ❌ never cache it in a static
  of your own. The TYPES are `pub` (a host implements them); slots and accessors are `pub(crate)`.
- **❌ Nothing here lowers thread QoS, and the runtime has no bearing on it.** The heavy walking / writing / reconciling
  work runs on **dedicated** `std::thread`s calling `cmdr_fs::thread_qos` in their own bodies; a class sticks to a
  thread for life, so it can never be set on a pooled tokio worker. `DETAILS.md` § "The runtime seam and thread QoS".
- **❌ Never consult `HostPolicy` on a per-entry path.** It returns a cheap `Copy` value precisely so a caller takes ONE
  snapshot per batch (a listing top-up, a between-images gate, a resume poll). Wanting a per-entry question means the
  call needs hoisting. `pace_tests::the_policy_is_consulted_per_listing_not_per_entry` pins it with a counting fake.
- **`priority_roots` is an ORDER, never a scope.** All the index may conclude is what to walk FIRST. ❌ Don't cache it
  as stable or let an empty answer stop a walk.
- **A `WorkClearance` field is a decision, never a timestamp**, so "how long counts as quiet" has one owner. Read
  `volume_idle` when you contend for one share's connection (a network scan), `app_idle` when you contend for the
  machine (image enrichment). Mixing them up is how browsing locally throttles a NAS scan.
- **`runtime::spawn` resolves a handle; `tokio::spawn` inherits one.** Indexing and the watcher can start from the app's
  synchronous `setup()` hook, where there's no ambient runtime, so `tokio::spawn` panics. Spawn through the seam.
- **❌ Never read a setting or resolve the data dir here.** The app turns stored settings into an `IndexConfig` and
  `set_config` applies it. The `CMDR_*` env knobs are the one exception (developer diagnostics).
- **Every seam degrades, none panics.** No provider ⇒ nothing mounted, no policy ⇒ nothing competing, no sink ⇒ events
  dropped, no config ⇒ `data_dir()` errors. Cases callers already handle, so tests and tools install nothing.
- **Vocabulary moves down; questions become seams.** `cmdr_fs::volume::{smb_volume_id, mtp_ids}` is pure string work, so
  it sits with the volume types: if you can compute it from a `&str`, it isn't a seam.

## Module map

- `runtime.rs` — the tokio runtime background work spawns onto (`set_runtime`, `spawn`, `spawn_blocking`, `block_on`).
- `policy.rs` — "may background work run now?" plus "what has the user's attention?": `HostPolicy`, the `Copy`
  `WorkClearance`, `OpenListing`, `priority_roots`, `AlwaysClear`, the test `FakeHostPolicy`. App side:
  `priority::host_policy::AppHostPolicy`.
- `volumes.rs` — what's mounted, where, what kind of storage, plus the SMB upgrade and MTP handle resolution:
  `VolumeProvider`, `MountFacts`, `NoVolumes`, and the test `FakeVolumeProvider`. App side:
  `file_system::index_provider::AppVolumeProvider`.
- `config.rs` — `IndexConfig` in, no settings reads: the data dir plus the media policy. App side:
  `commands::media_index::index_config_from`.
- `events.rs` — the injection point for the `EventSink` from `../events/sink.rs`. App side:
  `events::index_mapping::TauriEventSink`.

Rationale, the fallback runtime, and the QoS argument in full: `DETAILS.md`.
