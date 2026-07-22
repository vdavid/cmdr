# Indexing integration + stress tests

Cross-module tests that exercise the whole pipeline (scan → aggregate → enrich → watch) or hammer it under load. Unit
tests stay colocated in each module; these are the integration tier.

## Module map

- **integration_tests.rs** — end-to-end (scan → aggregate → enrich → watcher update → re-enrich), the enrichment fast
  path / fallback / root-level path, the `ReadPool` (reuse, invalidation, cross-thread, contention), the
  `should_auto_start_indexing` FDA gate, and the `IndexPhase` lifecycle transitions.
- **stress_tests_{concurrency,lifecycle,partial_aggregation}.rs** + **stress_test_helpers.rs** — concurrency,
  start/stop/restart-under-load, and the partial-aggregation differential test, plus shared setup helpers.
- **external_drive_fixture.rs** — a macOS-only synthetic disk-image FIXTURE (`#[cfg(target_os = "macos")]`), NOT a test
  file. Its FSKit-panic-safe attach/detach discipline is load-bearing.

## Must-knows

- **⚠️ FSKit kernel-panic guardrail: never mount, unmount, or probe a physical removable card in a test or a research
  script.** On 2026-07-15 a `diskutil unmount` on a physical, nearly-full FAT32 SD card wedged macOS 26's userspace
  FSKit `msdos` service mid-unmount; it held kernel vnode locks until the pile-up blocked WindowServer and the watchdog
  **kernel-panicked and rebooted the machine**. The wedge happens DURING unmount, so no post-unmount hook can undo it —
  the only defense is to never trigger it. Every external-drive test uses a disposable synthetic disk image through
  `external_drive_fixture`. **Attach once, detach once; never cycle mount/unmount, never `diskutil unmount` a path.**
  Every `hdiutil` call is hard-timeout-guarded (30 s → SIGKILL). ❌ Don't "clean up" the timeouts or the single-detach
  discipline; they're the guardrail against the incident.
- **Tests serialize on a dedicated mutex.** `INDEX_REGISTRY` is a global; concurrent tests corrupt each other. The
  pattern (in `integration_tests.rs` and `state/tests.rs`): a dedicated guard mutex + an `IndexStore` fixtured via
  `tempdir`, clearing the `root` entry AND the root read-path globals before and after. ❌ NEVER `INDEX_REGISTRY.clear()`
  in a test: it wipes every OTHER module's concurrent private instances (an isolation flake); remove only your own ids.
- **A test that asserts on pending-sizes / read-pool / `dir_stats` state must route through a PRIVATE per-volume
  instance, never the root `PENDING_SIZES` / `READ_POOL` globals** (foreign root writers clear those under bare
  `cargo test`). `stress_test_helpers::TestInstanceGuard` (the shared home) registers one under a unique id and removes
  it on drop; `register_identity_paths` gives an `mtp-` id whose read side maps plain `/paths` identically, so
  `get_dir_stats_on_volume` / `enrich_*_on_volume` work privately. Rationale: `writer/DETAILS.md` § "Test isolation".
- **"Disabled" is the absence of an instance.** There's no `IndexPhase::Disabled`, so assert `!contains_key` (or
  `get_read_pool_for(vid).is_none()`, the read-path "is it indexed?" predicate), never "phase is Disabled".
- **The external-drive tests are `#[ignore]`d and serialized** via the `disk-image` nextest group (`.config/nextest.toml`,
  30 s cap); `pnpm check rust` compiles them but the default suite skips them. Concurrent attach/detach churn on one
  FSKit service is the very surface the incident warns about.

The test inventory, the state-machine testing bar, and the disk-image fixture mechanics: [DETAILS.md](DETAILS.md). Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
