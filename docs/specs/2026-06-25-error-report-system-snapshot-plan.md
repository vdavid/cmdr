# Error-report system snapshot

Created 2026-06-25. Status: shipped. Implemented in `src-tauri/src/diagnostics_snapshot.rs`.

## Goal

Attach a richer system snapshot to our diagnostic bundles, matching (and beating) the "System" tab Commander One ships
with each report: macOS version, Mac model, RAM, CPU counts, language, thermal state. Plus two Cmdr-specific signals a
generic tool can't give: Cmdr's own memory footprint and the on-disk size of the drive index. The point is faster
triage: when a user reports "indexing is eating my disk" or "Cmdr feels sluggish," the bundle should already answer "on
what machine, with how much RAM/disk headroom, and how big is the index."

Privacy is the hard constraint: leave out hostname and anything else that fingerprints the person or names their drives.
This is non-negotiable and shapes several field choices below.

## What we already have (don't rebuild)

- `platform::os_version()` — `sw_vers -productVersion` → `"macOS 26.0"`. Already in `BundleManifest` and `CrashReport`.
- `std::env::consts::ARCH` — `"aarch64"` / `"x86_64"`. Already shipped as `arch`.
- `system_memory::get_system_memory_info_inner()` →
  `SystemMemoryInfo { total_bytes, wired_bytes, app_bytes, free_bytes }`, the accurate Mach `host_statistics64`
  breakdown that powers the Settings → AI → Local AI RAM gauge. This is the "elaborate" memory info to tap — reuse it
  verbatim, don't duplicate the Mach call.
- `sysinfo` crate is already a dependency (`Cargo.toml`, `system` feature).
- `indexing::retention::enumerate_external_index_dbs(data_dir)` already walks `index-*.db` files in the app data dir —
  reuse it for index sizing. Index DBs live at `<app_data_dir>/index-{volume_id}.db` (+ `-wal` / `-shm` siblings).
- `crate::config::resolved_app_data_dir(&app)` — the data dir holding the index DBs and logs.

## Field plan

Commander One's fields, mapped to our decision (✅ include, ⏭️ skip, 🔒 privacy note):

- **macOS Version** (`26.5.1 (Build 25F80)`): ✅ keep + add build number. `sw_vers -productVersion` (have) plus
  `-buildVersion` (new, one extra shell-out).
- **Mac Model** (`Mac15,9`): ✅ add. `sysctl hw.model` (one `libc::sysctlbyname`).
- **Memory (MiB)**: ✅ add (as bytes). `SystemMemoryInfo.total_bytes`.
- **CPU Type** (`ARM64`): ✅ have. `arch`.
- **CPU Speed (MHz)** (`-1`): ⏭️ skip. Unavailable on Apple Silicon — the screenshot shows `-1`.
- **Number of CPUs / Active CPUs**: ✅ add. `sysinfo` physical + logical core counts.
- **CPU is 64-Bit** (`YES`): ⏭️ skip. Always true on supported Macs.
- **Preferred Language** (`en-US`): ✅ add 🔒. The app already resolves its locale; mildly fingerprinting in aggregate,
  but coarse and useful for i18n prioritization.
- **Host Name** (`davids-m3-mbp.local`): ⏭️ skip (**PII**). Often contains a real name.
- **Thermal State** (`Nominal`): ✅ add. `NSProcessInfo.thermalState` (small objc2 bridge).

### Cmdr-specific additions (the genuinely useful part)

- ✅ **System memory breakdown** — ship the full `SystemMemoryInfo` (total / wired / app / free), not just total. Same
  data the Local AI gauge shows; tells us whether the machine was under memory pressure.
- ✅ **Cmdr's own RSS** — the app's resident memory via `sysinfo` process refresh (`System::new`, `refresh_process` for
  our pid, `process.memory()`). This is the "is Cmdr the problem" signal a system-wide gauge can't give.
  (`SystemMemoryInfo` is whole-machine; RSS is us.) Worth having both.
- ✅ **App uptime** — seconds since process start. Distinguishes "crashed on launch" from "leaked over 3 days." We
  already compute `uptime_secs` for `CrashReport`; lift it to a shared helper.
- ✅ **Drive index size** — total bytes of all `index-*.db` (+ `-wal`/`-shm`) in the data dir, **plus an unlabeled
  per-volume breakdown** (a list of byte sizes, one per index DB, sorted desc, **no names**). 🔒 The unlabeled list
  shows index skew without naming drives, staying within the error-reporter "never widen what we send / no volume names"
  rule.
- ✅ **Index-volume free space** — free / total bytes of the volume holding the data dir (`statfs` on the data dir). If
  indexing fails because the disk is full, this is the answer. Numbers only, no path (the data-dir path is a standard
  `~/Library/...` location and isn't shipped).

## Where it goes

Add one shared type, reused by both diagnostic flows (single-source, per `ideal-over-cheap`):

As shipped, the volatile fields are nested under `live`, present only for error reports. Crash reports are assembled at
next launch, where live values would describe the fresh process — so they get the stable fields with `live: None`
(honest over misleading).

```rust
// src-tauri/src/diagnostics_snapshot.rs
pub struct SystemSnapshot {
    pub os_build: Option<String>,            // sw_vers -buildVersion
    pub mac_model: Option<String>,           // sysctl hw.model
    pub cpu_physical: u32,                    // sysctl hw.physicalcpu
    pub cpu_logical: u32,                     // sysctl hw.logicalcpu
    pub preferred_language: Option<String>,
    pub total_memory_bytes: u64,             // stable; valid even at next-launch
    pub data_volume_free_bytes: Option<u64>, // statfs on the data dir
    pub data_volume_total_bytes: Option<u64>,
    pub index_total_bytes: u64,
    pub index_db_sizes: Vec<u64>,            // unlabeled per-volume sizes, sorted desc, NO names
    pub live: Option<LiveSystemState>,       // Some for error reports, None for crashes
}
pub struct LiveSystemState {
    pub thermal_state: Option<String>,       // nominal | fair | serious | critical
    pub memory: SystemMemoryInfo,            // reuse existing type (wired/app/free)
    pub process_rss_bytes: u64,              // Cmdr's RSS
    pub uptime_secs: f64,
}
```

- **Error reports**: add `system: SystemSnapshot` to `BundleManifest` (`error_reporter/mod.rs`). It serializes into
  `manifest.json` inside the zip — **no api-server change needed** for a human triager to read it. Build it in
  `bundle_builder::build_bundle` (healthy context, off the hot path, alongside the existing `os_version()` call).
- **Crash reports**: fold the same snapshot into `CrashReport`. ⚠️ Collect it only on the **next-launch signal assembly
  path**, NOT inside the panic hook / signal handler (compromised context — no sysctl/sysinfo/shell-outs there). For the
  panic path, the snapshot is assembled at next-launch when the on-disk crash file is processed, where the full stdlib
  is safe.

## Frontend / server touch points

- `BundleManifest` TS interface in `apps/desktop/src/lib/tauri-commands/error-reporter.ts` — mirror the new `system`
  field so the preview dialog can render it (and so the user sees exactly what's attached, which is the right
  transparency posture).
- The preview dialog (`src/lib/error-reporter/`) — optional: show a "System info" summary row so the user sees what
  ships. Nice-to-have, not required for the data to be useful.
- **api-server / dashboard**: zero changes for the basic win — the snapshot rides inside `manifest.json` in the zip.
  Only if we later want to _aggregate_ a field (e.g. "RAM distribution of crashing users") do we add it to R2
  `customMetadata` (`error-report.ts` `ErrorReportMeta`) and the dashboard's `ErrorReportRow`. Defer until there's a
  concrete question to answer.

## Privacy checklist (gate before merge)

- [x] No hostname.
- [x] No volume names (per-volume index sizing is an unlabeled byte list).
- [x] No paths in shipped numbers (data-dir free space is bytes only).
- [x] Preferred language is the only fingerprint-ish field; confirmed acceptable for a coarse locale signal.
- [x] Snapshot honors the existing "never widen what we send" rule (guardrail added to `error_reporter/CLAUDE.md`).

## Cost / risk

- ~1 hour for the error-report path. Collection is all fast local calls (sysctl/sysinfo/statfs, ms-scale); it runs where
  `os_version()` already shells out, so no new hot-path concern.
- Thermal state needs a tiny objc2 bridge (`NSProcessInfo`); it's the only non-trivial piece. If we want to defer it,
  everything else lands without it.
- TDD: unit-test `SystemSnapshot` serialization and the index-sizing helper (point it at a temp dir with fake
  `index-*.db` files); the live system values are environment-dependent, assert shape/non-negativity like
  `system_memory.rs` already does.

## Decisions (locked 2026-06-25)

- Index sizing: **total + unlabeled per-volume list** (`index_db_sizes`, sorted desc, no names).
- Apply to **both** error reports and crash reports (crash via next-launch assembly).
- **Keep** preferred language.
