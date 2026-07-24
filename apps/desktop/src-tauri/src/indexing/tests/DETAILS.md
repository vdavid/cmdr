# Indexing tests details

Read this before any non-trivial work in `indexing/tests/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

## How to test

Run Rust tests:

```sh
cd apps/desktop/src-tauri && cargo nextest run indexing
```

Tests use temp dirs and real SQLite. What each file covers:

- **integration_tests.rs** — end-to-end integration (scan → aggregate → enrich → watcher update → re-enrich), the
  enrichment fast path, fallback, root-level enrichment, the `ReadPool` (reuse, invalidation, cross-thread, contention),
  the `should_auto_start_indexing` FDA gate, and `IndexPhase` lifecycle transitions.
- **stress_tests_concurrency.rs** — concurrency stress: concurrent scan + replay, concurrent batch inserts, concurrent
  scan + enrichment reads, live event storm + reads.
- **stress_tests_lifecycle.rs** — lifecycle stress: start/stop/restart under load, clean lifecycle, double-start guard,
  early shutdown, rapid cycles, mixed queued work shutdown, and `disconnect_storm_writer_drain_holds_across_volumes`
  (20× spawn/write-burst/shutdown across two volume DBs via the non-search-feeding `spawn_for(.., false)` path, asserting
  no panic/lock/corruption under churn). The registry-level twin
  `disconnect_storm_two_volumes_never_wedges_the_registry` lives in `../lifecycle/state/tests.rs`.
- **stress_tests_partial_aggregation.rs** — the partial-aggregation differential test: the same synthetic tree fed to
  two writers (final-only vs partial-passes-interleaved), both compared after the final aggregation. The primary oracle
  is `check_db_consistency`'s independent recompute-from-`entries` on the partial-pass arm (it catches a maps-corruption
  bug an (a)==(b) comparison would miss, since corruption poisons both arms identically), with
  `check_recursive_has_symlinks` and the row-for-row (a)==(b) comparison as secondary oracles.
- **stress_test_helpers.rs** — shared helpers (`setup_writer`, `build_synthetic_tree`, `check_db_consistency`,
  `make_file_entry`, plus `build_synthetic_tree_with_symlinks_and_hardlinks` — injects symlink rows + a hardlink pair
  with the secondary link's sizes `None` — and `check_recursive_has_symlinks`, kept separate from `check_db_consistency`
  to keep the shared helper's blast radius small).

Colocated unit tests (Store, Aggregator, Scanner, Firmlinks, Writer, and the retention `select_evictions` LRU) live in
each module, not here.

## Testing bar: state machine + IndexPhase lifecycle

The per-volume `IndexPhase` lifecycle (`(absent) → Initializing → Running`, plus the `Initializing → removed` race
during cancel) is the trickiest backend state machine to test cleanly. Four rules:

1. **Tests must serialize on a dedicated mutex.** `INDEX_REGISTRY` is a global; concurrent tests corrupt each other.
   Pattern: a dedicated guard mutex + an `IndexStore` fixtured via `tempdir`, clearing the `root` entry (and the root
   read-path globals) before and after.
2. **"Disabled" is the absence of an instance.** There's no `IndexPhase::Disabled`, so assertions read "no entry for the
   volume id" (`!contains_key`) rather than "phase is Disabled". Reserving a volume installs its pool, so
   `get_read_pool_for(vid).is_some()` is the read-path "is it indexed?" predicate (proven in
   `state::tests::read_pool_routing_tracks_registration`); `reservations_are_independent_across_volumes` pins that two
   volume ids reserve/release without corrupting each other and route to distinct pools.
3. **`Initializing { store: IndexStore }` carries non-`Clone` owned state.** Building fixtures is verbose. Where you'd
   like to test a pure transition without the owned data, extract a pure classifier (e.g. `is_initializing_phase`) and
   test that in isolation. Don't pretend to mock `IndexStore`.
4. **`start_indexing`'s `(absent) → Initializing → Running` happy path needs `tauri::AppHandle`**: currently not feasible
   in unit tests without enabling the `tauri/test` feature (meaningful compile cost). The classifier-extraction approach
   plus the hand-installed `Initializing` instance cover the race-decision logic; the rest stays under integration / E2E
   coverage.

See `docs/testing.md` for the project-wide testing playbook.

## Testing external drives (synthetic disk images only)

⚠️ **Never mount, unmount, or probe a physical removable card in a test or a research script.** On 2026-07-15 a
`diskutil unmount` on a physical, nearly-full FAT32 SD card wedged macOS 26's userspace FSKit `msdos` service
mid-unmount; it held kernel vnode locks until the pile-up blocked WindowServer and the watchdog **kernel-panicked and
rebooted the machine**. The wedge happens DURING unmount, so no post-unmount hook can undo it — the only defense is to
never trigger it.

So every external-drive test uses a **disposable synthetic disk image**, through `indexing::tests::external_drive_fixture`
(macOS-only, `#[cfg(all(test, target_os = "macos"))]`):

- `DiskImageFixture::attach(DiskImageFilesystem::Fat32 | ExFat, volume_name)` runs `hdiutil create` + `hdiutil attach
  -nobrowse` on a fresh temp image, parses the `/dev/diskN` node and `/Volumes/…` mount, and returns a guard.
- `mount_point()` is the mount; `populate_known_tree()` writes a fixed tree (nested dirs, sized files, and an **empty**
  file — the empty file matters because FAT/exFAT give it a sentinel inode that changes once content is written) and
  returns the entries for assertions.
- The guard's `Drop` detaches once — `hdiutil detach`, then a `hdiutil detach -force` fallback — so teardown runs even
  on panic or early return. **Attach once, detach once; never cycle mount/unmount, never `diskutil unmount` a path.**
- **Every `hdiutil` call is hard-timeout-guarded** (`run_hdiutil_guarded`, `HDIUTIL_TIMEOUT = 30 s`): past the deadline
  the child is SIGKILLed (`Child::kill` → `SIGKILL`), so a wedged FSKit service is killed, never awaited. ❌ Don't "clean
  up" these timeouts or the single-detach discipline — they're the guardrail against the incident above.

The tests are `#[ignore]`d (each attaches a real disk image via hdiutil), so `pnpm check rust` compiles them but the
default suite skips them; run them explicitly:

```sh
cd apps/desktop/src-tauri && cargo nextest run --run-ignored only -E 'test(indexing::tests::external_drive_fixture::)'
```

They're serialized and granted a 30 s cap via the `disk-image` nextest group (`.config/nextest.toml`) — concurrent
attach/detach churn on one FSKit service is the very surface the incident warns about, and the live-FSEvents probes
block on real delivery. Those probes pin the load-bearing fact that **live FSEvents fire on FAT/exFAT despite no
`.fseventsd` journal** (no `sinceWhen` replay, but a running watcher keeps an external index current). The human-run
reference probes live beside this fixture in `external-drive-probes/` (`fat32-probe.sh`, `fsevents-probe.swift`); the
Rust fixture is the automated form.

`fat32_mount_relative_scan_indexes_the_tree_with_sizes_and_null_inodes` is the one end-to-end test on a real `msdos`
filesystem: it drives `scanner::scan_volume` with the mount-rooted `IndexPathSpace` (MountRooted exclusion scope + FAT's
untrusted-inode flag, resolved from the real `detect_filesystem_for_path` as `local_external_index::classify` does) and
asserts the drive's own index holds the tree under `ROOT_ID` by mount-relative name with recursive sizes, and the FAT
inode nulled. Asserts are lower bounds (macOS adds AppleDouble `._*` sidecars on FAT). The full app-level lifecycle
(enable → scan → sizes → eject-safe stop → detach) is not an automated CI test — the scan pipeline is `AppHandle`-bound
(no mock-app harness) and driving `hdiutil` in CI is the deliberately-avoided panic-class op; it's validated live via
MCP against the running dev app instead.

For `platform_case_compare` in `store.rs`: proptests cover the comparator algebra (reflexive / antisymmetric /
transitive) and NFC≡NFD equivalence on macOS. Don't regress those; see `store/tests.rs` for the property statements.
