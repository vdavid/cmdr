# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else (architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## One process per data dir

Two Cmdr processes on one data dir corrupt the index (two writers seeding the same entry-ID counter).
`instance_lock.rs` makes that impossible: it takes an advisory `flock` on `<data dir>/.instance.lock` in
the `setup` hook, before any database opens, and exits with a native alert if another process already holds
it. Anything that relaunches the app against a live data dir (an updater path, a capture script, a test
harness) must let the old process exit, or wait out the lock's ~5 s retry window. Mechanism, rationale, and
the retry-window callers: `docs/tooling/instance-isolation.md` § Instance lock.

## What `lib.rs` is, and what it deliberately isn't

`lib.rs`'s `run()` is the wiring: the Tauri builder chain, the `setup` hook's startup sequence, and the names of the
handlers. It is the ONE place a reader can see startup order, and order here is load-bearing (the hosts before any
background work, the logger before the data-dir claim, settings before the menu bar). So the long linear run of
`x::init(app.handle())` calls stays: breaking it into "phases" would hide the sequence behind function names without
adding a boundary anybody owns.

What does move out is any block with a real owner elsewhere. Three live outside today:

- `logging::startup::init()`: resolve the log dir, read the two early settings, install the fern tree, sweep legacy files.
- `menu::install::at_startup(app, &settings)`: pin the UI language, build the bar, run the macOS AppKit passes, place `MenuState`.
- `app_lifecycle::{on_window_event, on_run_event}`: the two builder handlers, plus the shared `stop_background_services`
  all three shutdown routes take (main window closed, main window destroyed, process exiting; none implies the others).

The test for moving a block: it has a module that already owns the subject, and moving it doesn't hide an ordering
constraint. A block whose only home would be "startup, part 4" stays in `lib.rs`.

## The E2E build's launch mock (`open_mock.rs`)

Every action that hands a path to another app records into `crate::open_mock` under the `playwright-e2e` feature
instead of launching: `open_path` and `open_in_editor` (`commands/file_actions.rs`), and "open terminal here"
(`src/file_system/terminal.rs`). The suite creates files and opens them, and it can't close a TextEdit or terminal window,
so real launches would pile up unbounded across runs. It's crate-level rather than colocated with any one of them
because all three share the store the specs read back through `e2e_opened_paths`. Same shape as the clipboard mock
(`clipboard/mock.rs`): compiled only under the feature, so prod and dev binaries never link it, and it never touches
the OS.

## Which Apple APIs skip the main-thread rule

`CLAUDE.md` requires an `objc2::MainThreadMarker` for AppKit/Cocoa main-thread-only calls. These are thread-safe and
carry no such requirement, so demanding a marker for them would be busywork: NSURL resource values, `NSFileManager`,
`NSUserDefaults`, LaunchServices, Keychain, IOKit, and Mach.

## Where the app answers a subsystem's seams (`index_host.rs`, `volume_host.rs`)

Two subsystems live below the app and can't reach it: the index (`crates/cmdr-index/`) and storage backends
(`crates/cmdr-fs/src/volume/host/`). Each declares what it needs as traits, and each has ONE crate-root module here
that answers, called from `setup()` before anything can start background work or construct a volume. Keeping the
answers in one file per subsystem is the point: "what does the app owe the index?" has a single readable answer, and
adding a seam over there means adding a line here.

The two differ in shape, deliberately. The index is reached through a process-wide handle (`index_host::index()`), a
concession to the globals it grew up with. A backend instead takes a cheaply-cloned `VolumeHost` VALUE in its
constructor, so a test can build one with fakes and pass it in, and nothing needs an install-and-restore guard;
`volume_host::host()` is only where the app parks the one it built. Each adapter lives beside the subsystem that can
answer it, listed in `crates/cmdr-fs/src/volume/host/DETAILS.md` and `index_host.rs`.

**Only the frontend event channel needs a running app**, so `volume_host::host()` hands out the real wiring even before
`install()` and leaves that one seam (plus the app's runtime) unanswered. Everything else — the listing cache, the
secret store, the index handle, the priority tracker, the settings module — is process-global and works in a test
binary. That's what lets an app-side backend test drive a real volume and then assert on the real listing cache
without standing a Tauri app up; a test that wants FAKES builds its own host instead.

## Number types over IPC (`ipc.rs`, specta bindings)

Tauri's IPC serializes through JSON, so the generated `bindings.ts` never sees a JS `bigint`.

- **Large integers.** `u64` / `i64` / `usize` / `isize` reach the frontend as TS `number` (`bigint` appears nowhere in
  `bindings.ts`), because a JS `number` truncates above 2^53. The values we actually send (file sizes, byte offsets,
  unix timestamps, counts) stay far below that ceiling, so `number` is honest here. If a command ever needs a genuinely
  huge integer (a raw inode, a hash, a nanosecond epoch), don't lean on this: give the field
  `#[specta(type = String)]` plus a serde `with` and parse it on the frontend.
- **A non-`Option` float can still arrive as `null` at runtime.** `serde_json` serializes `NaN` / `Infinity` /
  `-Infinity` as JSON `null`, so a Rust `f32` / `f64` return value that goes non-finite reaches the frontend as `null`
  even though `bindings.ts` types it `number`. The types don't express this, so keep the value provably finite on the
  Rust side (guard the division, clamp the result) rather than relying on the TS signature. This is a latent hazard,
  not an observed one: no crash or error report has shown a non-finite float crossing IPC.

**Decision: the specta trio stays pinned at `tauri-specta`/`specta` `=2.0.0-rc.24` and `specta-typescript` `=0.0.11`.**
rc.25 types every plain `f32` / `f64` as `number | null`. For *return* values that's arguably more honest (see the
`NaN` note above), but it applies the same rule to *parameters*, where it's simply wrong: `viewer_get_lines`
(`target_value: f64`) and the four `media_index_*_threshold` commands take non-`Option` floats, and serde rejects a
JSON `null` for those. Adopting rc.25 would trade a latent, never-observed hazard for a live one the frontend could
trigger by passing `null`, plus ~25 sites of null-handling. Renovate is disabled on all three (`renovate.json`);
re-evaluate on the next rc and bump all three together or not at all.

`bindings.ts` is generated: change this behavior at the `builder()` call site and regenerate with
`pnpm bindings:regen`, never by hand-editing the output.

**A test-only cargo feature must not move the exported surface.** The regen compiles under the feature set every cargo
check lane shares (`cmdr/virtual-mtp`), so it reuses their `target/` artifacts instead of rebuilding `cmdr` to answer a
different question. A command registered only under such a feature therefore has to be held back from its specta
collector while the crate compiles its own tests, which is where `export_bindings_test` writes the file — the
manifest's `typed unless cfg(test)` group in `ipc.rs` shows the shape, and E2E reaches those commands by raw
`__TAURI_INTERNALS__.invoke` rather than through the typed bindings. Runtime dispatch is untouched: that's a separate
`tauri::generate_handler![]` list. Why the lanes share one feature set: `scripts/check/checks/DETAILS.md` § "One feature
set across the cargo lanes".

The Ask Cmdr bulk-rename review commands register in the same builder and type collector as every other typed command.
Their authority and filesystem behavior live with the agent and write-operation modules, not at this registration edge.
