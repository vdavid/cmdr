# Crash reporter: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the crash-file
lifecycle and the exact payload catalog.

## Two delivery paths, sorted by survival

A panic can kill the app or not, and the hook fires identically either way: at panic
*initiation*, before unwinding, where nothing yet knows which it will be. So we don't
classify. We run both paths and let time do the sorting.

- **Disk → next launch.** The hook writes `crash-report.json` synchronously. For a fatal
  panic this is the only thing that outlives the process, so it goes first and it's
  unchanged: no thread spawn, no logging, no network in front of it.
- **Courier → this session.** The hook then hands a `PanicNotice` to a short-lived thread
  (`panic_courier.rs`) that logs the panic (error line + the panicking thread's backtrace
  as a debug record, the same shape `log_error!` emits) and calls
  `error_reporter::auto_dispatcher::on_error_logged`. That opens a Flow B window, which
  fires 60 s ± 10 s later.

**The debounce IS the fatal/non-fatal test.** A panic that kills the app kills it in
milliseconds, so the flush task is dropped and nothing ships in-session; the crash file
covers it at the next launch. A panic the app survives is still there 60 s later, so it
ships. No thread-name sniffing, no survival timer of our own, and no way for the two to
disagree about which case they're in.

### Why the courier, and not a call from the hook

Two independent reasons, either of which is fatal on its own:

1. **A panic raised inside a panic hook aborts the process**, and `catch_unwind` in the
   hook cannot stop it. `std::panicking` flips a thread-local `in_panic_hook` bit around
   the hook call; the next `panic!` on that thread returns
   `panic_count::MustAbort::PanicInHook` and calls `abort()` *before* unwinding starts, so
   there is nothing to catch (verified by reading `std::panicking` in the Rust 1.97.1
   source, 2026-08-23). Calling `log` or the dispatcher inline
   would mean a bug in the reporting path turns a survivable panic into a hard crash: the
   exact opposite of the point.
2. **A `std::sync::Mutex` the panicking thread already holds would self-deadlock.** The
   hook runs before unwinding, so the panicking thread still owns every guard it held.
   `log`'s internals and the dispatcher's `STATE` are both mutexes; re-locking one on the
   same thread hangs the app forever, and no poison recovery helps (there's no poison yet).

On a second thread both problems evaporate: a fresh panic count means `catch_unwind`
behaves normally, and lock contention is a wait of microseconds until the panicking thread
unwinds and drops its guards. The courier's whole body sits inside that `catch_unwind`.

### Rate limiting and reentrancy

One courier at a time (`COURIER_RUNNING`). That single flag is also the reentrancy guard:
if the courier itself panics, the hook re-enters `notify` **on the courier's own thread**
and finds the flag set, so there's no runaway. A panic storm costs one short-lived thread
at a time, and each courier's log line goes through the log coalescer. `Builder::spawn` is
used rather than `thread::spawn` because it reports failure as an `Err` instead of
panicking, which inside the hook would abort.

### What in-session delivery does and doesn't change about privacy

Nothing new leaves the machine, and nothing leaves it sooner than the user agreed to:

- The send is `on_error_logged`'s call, and it returns on the `updates.errorReports`
  opt-in check before touching the dispatcher state. Opt-in off means the panic is logged
  locally and that's all.
- The payload is the Flow B log-tail bundle that already ships, and Flow B bundles already
  carry Rust backtraces (`log_error!` emits one per error). The panic message rides the
  same `sanitize_panic_message` pass that guards the disk write, so the in-session copy
  can't be less redacted than the file.
- A user who opted into crash reports but not error reports sees exactly today's behavior.

### Known limitation: a survived panic can be reported twice

The crash file is written before anyone knows the app will live, and nothing deletes it
when the in-session report goes out. A user with **both** opt-ins on therefore gets an
error report now and a crash report at the next launch for the same panic. They land in
different channels (Discord vs the crash email) and carry different payloads (log tail vs
backtrace), so it's noise rather than a correctness problem, and the alternatives all cost
more than they're worth: deleting the file loses the backtrace for crash-reports-only
users, and marking it non-fatal needs next-launch UI copy for a case the dialog doesn't
describe.

Related and pre-existing: the next-launch dialog says "Cmdr quit unexpectedly last time",
which is untrue for a survived panic. That's a copy question, not a code one.

### Panics before the hook exists

`install_panic_hook` runs at the top of `run()`, so it covers all of startup. It still
can't write a file until `init` sets `CRASH_PATH` (the data dir isn't resolved before
Tauri's `setup`), and the logger isn't up that early either, so a panic in the first few
milliseconds of `run()` reaches stderr only. Everything after `logging::startup::init()`
is logged; everything after `crash_reporter::init` is also written to disk.

## Crash file lifecycle

1. App crashes; the handler writes `crash-report.json`.
2. Next launch: `check_pending_crash_report` finds the file and parses it defensively (discards if corrupt).
3. If `updates.crashReports` is `true` and it's not a crash loop: auto-send and show a toast.
4. Otherwise: show a dialog letting the user inspect and choose to send or dismiss. Radical transparency: the dialog
   shows the exact JSON payload before sending.
5. The file is deleted after send or dismiss.

## What we send

- Full symbolicated backtrace (function names + offsets, not file paths).
- Exception type + signal, faulting address.
- App version, macOS version, CPU architecture.
- App uptime, thread count.
- Sanitized panic message (`panicMessage`): redacted through `crate::redact`, then capped at
  `PANIC_MESSAGE_MAX_CHARS` (2,000) with a `… (truncated)` marker. `None` for signal crashes, which carry no payload.
  The cap exists because the ingestion endpoint rejects a report body over 64 KB, so an uncapped `assert_eq!` dump of a
  big struct would cost the whole report instead of its own tail. The api server caps again on its side.
- Active feature flags (booleans/enums only: `indexing.enabled`, `ai.provider`, `developer.mcpEnabled`,
  `developer.verboseLogging`).
- `buildMode` (`"release"` or `"debug"`, from `cfg!(debug_assertions)`): lets the api server distinguish dev-run crashes
  from production ones in the email summary.
- `shortId` (`CRASH-XXXXX`): generated at crash-file-write time via `crate::short_id::generate("CRASH")` (shared
  alphabet with error reports). Shown to the user in the next-launch dialog so they can reference the report.
- `diagId` (`diag_<uuid>`): the diagnostics id from `crate::install_id`, so sequential reports from one install group
  together. See the `CLAUDE.md` invariant on why this is never the `anal_` analytics id and is attached at assembly
  time, not in the signal handler.
- `email` (optional): a beta tester's contact email, populated only by the dialog at send time when the user ticks the
  attach-email box. The dialog threads it into `send_crash_report(report)`.
- `systemSnapshot` (optional): the stable machine snapshot from [`crate::diagnostics_snapshot`] — Mac model, CPU counts,
  OS build, total RAM, the data-dir volume's free/total bytes, and drive-index sizes (total plus an unlabeled
  per-database list). Attached at next-launch assembly in `process_pending_crash`, never in the panic hook or signal
  handler; `live` is always `None` for crashes (see the `CLAUDE.md` invariant). PII-free: no hostname, paths, or volume
  names.
- `imageBase` (optional): the main executable's load address at crash time, as `"0x…"`. See § Image base.

## Signal-handler limits we accept

- **The pre-opened fd goes stale if the data dir is deleted while the app runs.** Acceptable: it takes deliberate user
  action and loses at most one report.
- **`backtrace()` from `execinfo.h` is async-signal-safe on macOS.** On Linux, glibc's is safe in practice but not
  POSIX-guaranteed; the Linux E2E container is the only place we run it.

## Image base

Signal-crash `backtraceFrames` are absolute virtual addresses, and ASLR re-slides the binary on every launch, so on
their own they can't be compared between two launches, let alone two users. `imageBase` is the missing half:
`frame - imageBase` is a stable per-build offset, which makes identical crash sites group across installs and lets
`atos -o <binary> -l <imageBase> <frame…>` resolve them wherever that build's symbols are available.

- **Resolved at `install()` time**, via `_dyld_get_image_header(0)` (index 0 is the main executable), and stored in an
  `AtomicU64`. The handler only does an atomic load, which is async-signal-safe; the dyld call is not, so it must never
  move into the handler.
- **The signal path reports the base recorded in the raw crash file, never the current process's.** The report is
  assembled after relaunch, and that process has a different slide, so using it would make every offset wrong. The panic
  path is the opposite case: it runs in the crashing process, so it reads the live value.
- **Raw crash file format is v2**: header (magic, version, signal, frame count) then the 8-byte base, then frames, then
  the padded app version. A v1 file fails the version check and is discarded, which is fine (raw files never outlive the
  next launch).
- **PII-free by construction**: a randomized virtual address, no user data. Deliberately only the numeric base and
  **never a loaded-image path list** — macOS's own `.ips` includes those, and they embed `/Users/<name>`.
- `None` on non-macOS Unix (no `_dyld_*`), and for reports written before the field existed.
- Symbolication needs the matching build's symbols, but that most likely does NOT mean archiving dSYMs. There's no
  `[profile.release]` override, so cargo's defaults apply (`debug = false`, no `strip`): the shipped binary should still
  carry its symbol table, and every released binary is kept by definition since it's published. So
  `atos -o <released binary> -l <imageBase> <frame…>` should already resolve function names (no file/line). **Verify
  this on the next release** with an `nm`/`atos` smoke test against a known address; if Tauri's bundler turns out to
  strip the binary, archive the UNSTRIPPED BINARY per release, which is far smaller than dSYMs and needs no profile
  change. Turning on `debug = true` for real dSYMs costs longer builds, hundreds of MB per release to retain for as long
  as old versions run, a symbolication step to build, and a change to the signed/notarized release pipeline, all to add
  line numbers.
- **Scope check before investing in any of that.** The panic path already carries real function names (the hook captures
  `std::backtrace`), so symbols only matter for the SIGSEGV/SIGBUS/SIGABRT path, and only for frames in our own compiled
  code. Native crashes commonly land almost entirely in system frameworks, where our symbols wouldn't help either way:
  the WebKit teardown crash in `docs/notes/child-window-close-webkit-crash.md` had ~23 WebKit/AppKit frames and only
  event-loop boilerplate of ours. For those, macOS's own `.ips` is the better tool.

## What we never send

- File paths, volume names, environment variables, window titles.
- Hostname, or any per-volume *names* in the index-size breakdown (sizes only, unlabeled).
- License key, transaction id, device id.
- Register dump, heap contents.
