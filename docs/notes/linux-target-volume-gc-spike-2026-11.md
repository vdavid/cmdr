# Linux target volume GC spike (2026-11-12)

Measured 2026-11-12 in OrbStack, image `cmdr-rust-linux:f023f64c61a2`, cargo 1.99.1, against throwaway btrfs reflink
clones of the main clone's volume `cmdr-rust-linux-target-cmdr-32b84a01-1.99.1` (92 GiB;
`scripts/check/checks/DETAILS.md` calls ~18.6 GB healthy). Host load average was 18–92 throughout (other agents), so
timings are noisy.

Result: a mark-and-sweep at promotion gets the volume from 92 GB to roughly 15–30 GB with zero cold builds. Dropping
`incremental/` wholesale makes every new worktree's first clippy-linux run about 5x slower, so it's the wrong lever.

## 1. Cargo's own GC (cargo 1.99.1, 2026-08-27)

- Nothing for the target or build dir, stable or nightly.
  - `cargo help clean` only has whole-dir, `-p`, `--profile`, `--release`, `--doc`, and `--target` cleaning.
  - `-Z gc` covers the `CARGO_HOME` global cache only. Its `cache.global-clean` max ages key on cargo's last-use
    tracking database, and `cargo clean gc` is nightly. `cache.auto-clean-frequency` went stable in 1.88, also
    global-cache only.
- Upstream: target-dir GC is rust-lang/cargo#5026, open. #6229 was closed into it on 2026-08-13, and #13136 ("Garbage
  collect whole `target/`") is also open. epage on 2026-08-13: they're "preparing to stabilize a new build-dir layout
  that will unblock progress on #5026" (`-Z build-dir-new-layout`, nightly).
- `-Z mtime-on-use` is nightly-only. Forcing it on stable with `RUSTC_BOOTSTRAP=1` changes every unit's hash and rebuilt
  397 units: a dead end.

## 2. cargo-sweep

- 1,004 stars, not archived, latest release v0.8.0 (2025-10-11), last commit 2026-05-26. Its README says "currently
  unmaintained" and points to `cargo-clean-all`, which cleans whole projects, not individual units.
- It keys on the atime of files inside each `.fingerprint/<unit>/` dir (`src/fingerprint.rs`, `last_used_time` calls
  `metadata().accessed()`). `--installed` and `--toolchains` key on the rustc hash in the fingerprint JSON, which
  doesn't matter here because volumes are already per channel.
- The volume is btrfs mounted `noatime`, so atime stays at build time: `--time N`, `--maxsize`, and `--stamp`/`--file`
  all mean "built more than N ago", not "unused for N". That's dangerous. On the main volume, the live units by
  fingerprint build date were 2,186 from 2026-11-12, 5 from 2026-10-22, and 3 from 2026-10-28. The old live ones are the
  stable foundation crates nothing bumps; deleting one makes cargo rebuild everything above it (a rebuilt dependency
  marks its dependents stale; the cascade's size wasn't measured).

## 3. What's in the 92 GiB

- `debug/` 92 G, `doc/` 818 M, `cxxbridge/` 12 K. Only the dev/test profile, no old-toolchain leftovers (one
  `.rustc_info.json`; the channel is in the volume name).
- `debug/incremental`: 65 G. 700 crate-hash dirs, roughly nine generations of every workspace crate (`cmdr_lib` alone
  has 33 dirs, up to 3.4 G each). Each dir holds only one or two sessions, because rustc cleans up sessions inside a
  dir. The growth is new crate-hash dirs: a workspace crate's hash changes whenever something in its dependency graph or
  its version changes (Renovate waves, releases), and the old dir is never read again. Size by newest-session date:
  10-23 5.4 G, 10-28 4.1 G, 10-29 6.9 G, 10-30 12.5 G, 11-03 9.3 G, 11-04 13.3 G, 11-12 13.6 G.
- `debug/deps`: 25 G in 12,911 files, about 19 G stale: old generations of workspace test binaries (`cmdr_lib-<hash>`
  around 640 MB times six) and `libtauri_plugin_mcp_bridge-*.a` (340 MB times three).
- `debug/build`: 3.4 G, 854 dirs, 270 live. `.fingerprint`: 6,167 dirs, 2,211 live. `examples`: 468 M.

## 4. Experiments on reflink clones

Setup: clone A warmed against the main tree (clippy rebuilt 72 units in 96 s; the nextest build 61 units in 620 s at
load ~80; doc 62 units in 236 s; warm reruns 2–4 s, 3 s, 2 s). That grew A to 101 G. The live set came from the lanes'
cargo commands with JSON output: clippy, `nextest run --no-run --cargo-message-format json`, and doc with the lane's
`RUSTDOCFLAGS`.

- (a) Prune by the JSON hash set alone (clone B): not enough. Afterwards tests rebuilt 8 units in 46 s and doc
  re-documented 39 in 168 s. The missing pieces are workspace units whose hash appears in no output filename: the
  cdylib/staticlib `cmdr_lib` (unhashed outputs), bins and examples (the JSON names only the uplifted copy, a hard link
  of `deps/X-<hash>`), and doc units (output goes to `doc/`).
- (b) Exact prune of third-party units only (clone C). Rule: a non-workspace entry in `deps/`, `.fingerprint/`,
  `build/`, or `examples/` is deleted when its 16-hex hash isn't in the JSON set. Third-party outputs are always hashed.
  Workspace names come from `cargo metadata --no-deps`. Removed 11,231 entries: 101 G down to 90 G. Afterwards clippy
  rebuilt 0 (2 s), tests 0 (4 s), doc 0 (3 s). This part is exact.
- (c) Also prune workspace units and incremental dirs by mtime (clone D, cutoff 2026-11-12 00:00). Removed 1,460
  workspace entries and 493 incremental dirs: down to 31 G (incremental 21 G, deps 8.4 G, build 404 M). That 21 G is one
  heavy day (two promotions plus the spike's runs). Warm runs against the main tree: clippy rebuilt 4 (0.8 s), tests 4
  (6 s), doc 31 (2m24s): unchanged crates last built before the cutoff. This only costs anything on main-clone warm
  runs. A seeded worktree rechecks every workspace unit anyway because of mtimes (`scripts/worktree-hooks/DETAILS.md`),
  so old workspace rlibs are never reused there; only their incremental dirs help.
- (d) A new seeded worktree, the hot path. Tree extracted with `git archive` into a volume, `resources/ai/` copied,
  every file touched to now. clippy-linux's command on D (live incremental kept): 29 s. On E (D with `incremental/`
  removed): 2m39s. Both rebuilt the same 94 units, at load 18–42 rising during E. The incremental dirs from older
  worktrees are reused: the same crate-hash dirs got new sessions. clippy's live incremental set is 94 dirs: 4.9 GB
  including replaced sessions, 2.5 GB written fresh. The test-profile equivalent wasn't run.

So dropping `incremental/` at promotion turns every new worktree's first clippy-linux run from about 30 s into about 2.5
min, close to cold; a reset at a size budget adds a fully cold run on top.

## Recommendation

Mark-and-sweep at promotion, inside `copyScript` (`scripts/check/linux-cache/handoff.go`), on `/to/.incoming` before the
`mv --exchange`, so the worktree's own volume stays untouched and the swap stays atomic.

1. Third-party units (`deps/`, `.fingerprint/`, `build/`, `examples/`): keep an entry only if its hash is in the live
   set.
   - Each Linux lane writes its live hashes to `/cargo-target/.cmdr-live/<lane>.hashes` after it runs, overwriting its
     previous file. The files live inside the volume, so they travel with the seed and the promotion. A lane that never
     ran in the worktree still has its last list from the main clone's volume. Keep the union.
   - Getting the JSON: rustdoc-linux already uses `--message-format=json`. clippy-linux can switch to JSON and render
     diagnostics the way `rustdocOwnOutput` does, or run a warm follow-up `cargo clippy --message-format=json` (~2 s).
     The tests lane can add a warm `cargo nextest run --no-run --cargo-message-format json` (3–11 s).
   - NOT verified: that message format doesn't change fingerprints. The spike's JSON reruns were warm after JSON runs; a
     text run followed by a JSON run wasn't tested.
2. Workspace units (names from `cargo metadata --no-deps`): keep an entry if its hash is live, if it's a hard link
   (nlink > 1: a currently uplifted bin or example), or if it was written after the worktree's seed. `seed()` would
   write a `.cmdr-seeded-at` stamp; `cp -a` keeps mtimes, so the comparison holds. Fingerprint dirs are a few KB each,
   so a 30-day mtime rule is enough for them.
3. `incremental/`: keep a dir if one of its sessions is newer than the seed stamp or newer than 72 hours. The window
   protects the incremental dirs of lanes the worktree didn't run (often tests-linux, since local checks mostly run
   clippy-linux).
4. Backstop, in this order: if the volume is still over a 40 GB budget after the sweep, re-prune `incremental/` to
   "since seed" only; reset the volume only if it's still over budget after that (expected never).

Trigger: every promotion (several a day), plus the same sweep at seed time so a new worktree doesn't inherit stale dirs.
The sweep took about 5 s for 12,760 deletions on btrfs, inside the hook's 2-minute `copyTimeout`.

Expected steady state: ~5 G live third-party deps and build output, ~3–8 G live workspace output, 5–20 G incremental
depending on how many dependency or release generations the last 72 hours had. Roughly 15–30 GB.

Expected rebuild cost: zero cold builds. Third-party: no rebuilds (0 rebuilt units across all three lanes). Seeded
worktrees: unchanged. Main-clone warm runs: a few small workspace units, and rustdoc re-documenting crates unchanged for
more than 72 hours (2m24s at high load); keeping doc fingerprints regardless of age removes that. A lane idle for more
than 72 hours pays roughly the 29 s vs 2m39s gap once.

Risks:

- A lane's hash file missing or empty would delete live third-party units and cause a cascade rebuild. Guard: skip the
  sweep when any lane's file is missing or older than its last change to `Cargo.lock` or the toolchain.
- A workspace target name matching a third-party crate name gets classified as workspace. Mostly harmless: the workspace
  rules are more permissive.
- Depends on `<name>-<16hex>` entry names and the current layout. `-Z build-dir-new-layout` will change it when
  stabilized; pin a test against the layout.
- The 72-hour window doesn't bound a heavy day (21 G of incremental within one day here). The budget backstop covers
  that.
- Once rust-lang/cargo#5026 lands with real last-use tracking, replace all of this with it.

## Follow-up (2026-11-12, while building it)

- The one unverified assumption holds: message format never changes a fingerprint. On a warm clone (`sweep-dev-a`), a
  text `cargo clippy` run that rebuilt 36 units, then text, `--message-format=json`, `json-render-diagnostics`, and text
  again rebuilt 0 units each; the same sequence for `cargo nextest run --no-run` (text, `--cargo-message-format json`,
  `json-render-diagnostics`, text) rebuilt 0 each. cargo 1.99.1, nextest 0.9.136.
- What shipped, and where it departs from the recommendation above (raw JSON streams instead of hash lists, partial
  streams in place of the `Cargo.lock` staleness check, one rule set for every unit instead of a workspace/third-party
  split): `scripts/check/checks/DETAILS.md` § "The target volume sweeps itself", with the end-to-end numbers.
