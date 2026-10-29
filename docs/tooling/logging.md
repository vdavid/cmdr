# Logging

Both frontend and backend logs appear in a single unified stream (terminal + log file). The Rust side runs a hand-rolled
`fern` dispatch tree with **per-output level filtering**: file always at Debug, terminal defaults to Info.

## Frontend (Svelte/TypeScript)

Uses [LogTape](https://logtape.org/) with a batching bridge to the Rust backend.

### Usage

```typescript
import { getAppLogger } from '#lib/logging/logger'

const log = getAppLogger('fileExplorer')
log.debug('Loading directory {path}', { path })
log.info('Loaded {count} files', { count: files.length })
log.warn('Large directory: {count} items', { count })
log.error('Failed to load: {error}', { error: err.message })
```

### How it works

Logs are sent to the Rust backend via a batching bridge (`log-bridge.ts`):

- **Batching**: Collects entries for 100ms, sends in a single IPC call
- **Deduplication**: Consecutive identical `(level, category, message)` tuples collapse into one with a
  `(xN, deduplicated)` suffix
- **Throttle**: Max 200 entries/second; excess entries are dropped with a warning
- **Message only**: the bridge sends the rendered string. `FrontendLogEntry` carries level, category, and message, and
  nothing reads `record.properties`, so a field the template never names is dropped here and appears in no log file and
  no error-report bundle. `cmdr/no-unrendered-log-fields` fails the build on one.

Logs appear in both the **browser console** (via LogTape's console sink) AND the **terminal/log file** (via the bridge).

Because the message is the whole payload, write templates that render what you'd want at triage time:

```typescript
log.error('Trash refused it: {errorType}', { errorType: error.type, error }) // ❌ `error` is discarded
log.error('Trash refused it: {errorType} ({detail})', { errorType: error.type, detail: error.message }) // ✅
```

Pass a value raw (`{ error: e }`): the bridge renders every placeholder through `describeForLog`
(`apps/desktop/src/lib/logging/describe-for-log.ts`), which gives an `Error` as `Name: message` plus its cause chain, a
`TypedFailure` plus its payload, and an object as compact JSON. ❌ Don't pre-stringify: `String(e)` and the
`e instanceof Error ? e.message : String(e)` ternary drop the cause and payload, and `cmdr/no-stringified-log-values`
fails the build on them (it autofixes). `{*}` renders the whole property bag and satisfies the rule, but it's a blunt
instrument: prefer naming what matters.

Name a path or identity placeholder from `REDACTOR_KEY_BY_PLACEHOLDER` in `log-bridge.ts` (`{path}`, `{dir}`,
`{fileName}`, `{volumeName}`, `{server}`, …): the bridge writes it as a keyed field with the value marked by its kind (§
"Marked values"). Log a typed error as itself (`{error}`), never as prose built from its fields: `describeForLog` marks
its `path`, `name`, `source`, `deviceName`, and other identity values the same way.

### Log levels

From lowest to highest: `debug` < `info` < `warning` < `error` < `fatal`

### Default behavior

- **Dev mode (terminal)**: Shows `info` and above by default. `RUST_LOG` overrides per-module. The verbose toggle bumps
  the whole terminal to `debug` at runtime.
- **Prod mode (terminal)**: Terminal isn't visible in shipped builds.
- **File target**: Always `debug` when log storage is enabled (cap > 0), except `mdns_sd` at `info`. Absent when the cap
  is `0`. The file target is independent: it ignores `RUST_LOG` and the verbose toggle, so error report bundles always
  carry our full debug context. A report ships `debug` and `trace` only from our own crates (lines tagged `·`, see
  [Log format](#log-format)); `info` and above ship from anyone, redacted.

## Log levels

- **ERROR**: Something broke. Needs developer or user action.
- **WARN**: Unexpected but handled, or a degraded state worth noticing.
- **INFO**: Noteworthy lifecycle events. App started, operation canceled, errors that are part of normal functioning.
  Only a few selected interesting events, without noise.
- **DEBUG**: Routine operational details. Individual file copies, per-item MTP operations, intermediate steps. Disabled
  by default. Enable per scope with `RUST_LOG=module=debug` when investigating.
- **TRACE**: Protocol-level internals, full data structures, per-iteration details

### Keeping the file readable

The file target is always Debug so a bundle carries full context, which makes Debug volume a real cost: excessive
logging makes a log harder to read, not more informative. Two rules keep it honest.

**A repetitive line gets an aggregate, not a demotion.** Demoting a per-event line to Trace removes it from every
bundle, so the information is gone rather than compressed. Instead, count the events and put the number somewhere a
reader already looks:

- Fold the count into a summary line that already exists. `reconcile_subtree` counts the directories that vanished
  mid-walk and reports `143 unreadable dirs` on its own completion line; the churn line above carries the queued-signal
  count.
- Or roll the line up with `cmdr_fs::log_rollup::LogRollup`: the first occurrence logs immediately, then sustained
  repetition collapses to one line per window carrying `×N in Ys`. Keyed per volume or share, so a busy one can't
  swallow a quiet one's first line. Used by the space poller, the SMB space poll, the media live tick, and the
  importance rescore. Roll up only the case that says nothing: a pass that DID something keeps its own line.

**A heartbeat speaks when it has something to say.** The writer and reconciler stall probes keep full 5 s resolution
whenever there's work in flight (a queue with no progress is the stall they exist to show) and drop to one line a minute
while genuinely idle, which is enough to tell "idle" from "dead".

When you do demote a line to Trace, say in a comment why a bundle doesn't need it, and leave the per-path detail
reachable through a `RUST_LOG` recipe below.

### Enabling debug logs for a feature

Use `RUST_LOG` to enable FE debug logs in the terminal (no code changes needed):

```bash
RUST_LOG=FE:fileExplorer=debug,info pnpm dev
```

In a production build, frontend warn and above from every category reach the log file (and so error-report bundles);
info and debug don't. To make a feature's debug lines reach production logs too, add it to `debugCategories` in
`apps/desktop/src/lib/logging/logger.ts`:

```typescript
export const debugCategories: readonly string[] = [
  'fileExplorer', // Its debug lines now reach production logs and bundles
]
```

That doesn't change the browser devtools console, which keeps its own gate; the verbose toggle below lifts it. Gate
mechanics and why warn is the production line: `apps/desktop/src/lib/logging/DETAILS.md`.

## Backend (Rust)

Hand-rolled `fern` dispatch tree (see `apps/desktop/src-tauri/src/logging/CLAUDE.md`) with `RUST_LOG` parsed into
per-module overrides on the stdout chain only. Same `log` facade API.

### Usage in Rust

```rust
use cmdr_fs::log_detail::LogDetail;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use log::{debug, info, warn};

debug!(target: "listing", "listed path={:?} in {ms}ms", LogPrivate::dir_path(&path));
info!(target: "listing", "Loaded {} files", count);
warn!(target: "copy", "couldn't open {}: {}", LogPrivate::file_path(&path), LogError(&volume_error));
warn!(
    target: "smb",
    "tree connect failed share={:?} stderr={:?}",
    LogPrivate::volume(&share),
    LogDetail(&stderr).marking(&[LogPrivate::volume(&share)])
);
crate::log_error!(target: "copy", "Failed: {}", err);
```

Wrap every user-derived value (a path, a file or folder name, a volume or share, a host, an account) in its `LogPrivate`
kind (`path`, `file_path`, `dir_path`, `name`, `file_name`, `dir_name`, `volume`, `host`, `user`), keeping the site's
key and format spec; use the neutral kind when the site doesn't know which. Foreign text goes in a `LogDetail`, with
`.marking(&[…])` naming the values the caller handed over. An error type that carries a path (`VolumeError`,
`FriendlyGitError`, `VolumeScanError`, …) logs as `LogError(&e)`, which marks its paths; one that doesn't implement
`ErrorLogText` won't compile there. Error-level sites use `log_error!`. A `LogPrivate` is log text only: never turn one
into a `String`. How it works: § "Marked values" below and `crates/cmdr-fs/DETAILS.md` § "Marked log values". The
`log-private-values` check holds every log macro to it, strictly; a value that only looks like the user's (a setting's
name) opts out with `// allowed-log-private: <reason>`.

### Enable debug for specific modules

**Important**: The crate name in `Cargo.toml` is `cmdr`, but the Rust library target compiles as `cmdr_lib`. Always use
`cmdr_lib::` as the module prefix in `RUST_LOG`, not `cmdr::`. Using `cmdr::` silently matches nothing.

```bash
# Debug for network module only
RUST_LOG=cmdr_lib::network=debug pnpm dev

# Debug + suppress noisy SMB logs
RUST_LOG=cmdr_lib::network=debug,smb=warn,sspi=warn,info pnpm dev

# Trace everything (very verbose)
RUST_LOG=trace pnpm dev
```

## Log file

- **Location**: Prod: `~/Library/Logs/com.veszelovszki.cmdr/`, Dev:
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/`
- Contains both Rust and frontend logs
- **Level**: Always Debug for the file target. The dispatch tree filters per output, so the file's Debug level is
  independent of `RUST_LOG` and the verbose toggle (those only affect the terminal). Error reports always get full debug
  context.
- **Rotation**: 50 MB per file, keep-N where `N = ceil(cap_mb / 50)`. Our own
  `apps/desktop/src-tauri/src/logging/rotating_file.rs`. A writer whose file another process already rotated away
  reopens `cmdr.log` instead of rotating again, so two apps sharing the folder don't rename each other's live file.
- **Cap**: `Advanced > Maximum disk space for log files (MB)`, default 200 MB, range 0–5000. Set to `0` to disable log
  storage entirely. Error reports cannot be sent without logs. Lowering the cap at runtime eagerly prunes excess files.
  `0 ↔ non-zero` transitions (and raising the cap beyond its baked-in value) require an app restart.
- Open the current file read-only from **Help > View debug log** or **View debug log** in the command palette.
- Accessible from **Settings > Advanced > "Logging and diagnostics" > "Open log file"**, which reveals the log FOLDER in
  Finder rather than opening a file. Also bundled into error reports sent via **Help > Send error report…** (passes
  through the shared redactor first)

## Ask Cmdr chats (reading a past conversation)

The debug log doesn't hold Ask Cmdr transcripts. Every chat, with each tool call and tool result, lives in the agent
store `main.db`, so reading one needs no MCP and no running app:

- **Where**: Prod `~/Library/Application Support/com.veszelovszki.cmdr/main.db`, dev
  `…/com.veszelovszki.cmdr-dev/main.db`, a worktree `…/com.veszelovszki.cmdr-dev-<slug>/main.db`. WAL mode, so open it
  read-only while the app runs.
- **Find the chat**:
  `sqlite3 -readonly main.db "SELECT id, title, origin, datetime(updated_at,'unixepoch') FROM conversations ORDER BY updated_at DESC LIMIT 10;"`.
  Timestamps are UTC unix seconds; `origin = 'notification'` rows are wake threads.
- **Read it**:
  `sqlite3 -readonly main.db "SELECT seq, role, content_blocks FROM messages WHERE conversation_id = <id> ORDER BY seq;" > <scratchpad>/chat.txt`,
  then read the file. `content_blocks` is typed JSON (`text`, `tool_call`, `tool_result`); tool results can be large.
- **What it proposed**: the `propose_suggestions` tool call carries the groups as the agent sent them; the stored
  sweeps, groups, and ops are `proposal_sets` / `proposals` / `proposal_ops` in the same file
  (`apps/desktop/src-tauri/src/agent/store/proposals/CLAUDE.md`).
- **The exact prompts sent to the provider**: only when Settings has `logLlmCalls` on (dev default on, prod default
  off), under `{app data dir}/llm-logs/thread-<conversation id>/`. Format:
  `apps/desktop/src-tauri/src/ai/llm_log/CLAUDE.md`.
- `content_blocks` can hold the provider's opaque reasoning blob: fine to read locally, ❌ never paste it into an issue
  or a public file, and the same goes for the user's paths and file names (this repo is public).

## Log format

```
10:19:34.908 DEBUG indexing::writer  Writer: +412 msgs (390 upserts, 18 deletes, 4 flushes) in 5.0s [98,204 total]
10:19:34.912 INFO  FE:fileExplorer   Loaded 1,204 files
```

Format: `HH:MM:SS.mmm LEVEL target  message`. Frontend logs appear with an `FE:` prefix followed by the LogTape category
name.

The log FILE writes an ISO-8601 stamp with offset instead of the clock time, and a `·` right before the target of every
line our own code wrote (`2026-10-19T10:19:34.908+02:00 DEBUG ·indexing::writer  …`). Untagged lines come from a
third-party crate (`mdns_sd`, `reqwest`, …) or from a build older than the tag; error reports leave out their `DEBUG`
and `TRACE` (`apps/desktop/src-tauri/src/logging/provenance.rs`).

The terminal adds ANSI on top of that, and only when there's a terminal to add it for: `NO_COLOR` turns it off,
`CLICOLOR_FORCE` / `FORCE_COLOR` decide it outright (`0` off), and otherwise the app asks whether its own stderr is a
terminal. Under `pnpm dev` that last question is useless, because the Tauri CLI hands the app a pipe either way, so
`tauri-wrapper.ts` answers for it: it sets `CLICOLOR_FORCE=1` when the terminal you launched from is really there. So
`pnpm dev` colorizes, `pnpm dev 2> log.txt` doesn't, and `NO_COLOR=1 pnpm dev` doesn't. The target's head (everything
before its first `::`) is padded to 14 columns and painted a color derived from the head itself, so a subsystem keeps
the same color across runs and machines; the module path after it goes gray. The head column makes the terminal shape
`… INFO  downloads     ::watcher  Started watching …`. The log FILE is unaffected: no ANSI, no padding.

## Marked values

Every log site marks each user-derived value (a path, a name, a volume, a host, an account) between invisible Unicode
noncharacters from U+FDD0–U+FDEF (`crates/cmdr-fs/src/log_private.rs`), so an error report hashes exactly that value
instead of guessing where a name ends (`apps/desktop/src-tauri/src/redact/DETAILS.md` § "Marked spans"). Frontend lines
carry the same markers, written by the log bridge (`apps/desktop/src/lib/logging/DETAILS.md` § "Marking values for error
reports").

- **The terminal and Help > View debug log hide the markers** and show each marked value as itself (`Café`, not
  `Cafe\u{301}`). The viewer's search, copy, and "Save range" work on that shown text. The browser devtools console
  shows none either.
- **The log FILE keeps them**: the error reporter reads them from it. There, `less` shows a marker as `<U+FDD2>`, a
  marker that went through `{:?}` reads `\u{fdd2}`, and a value printed by a `{}` site carries its `{:?}` escapes
  (`Cafe\u{301}`), one escape layer more inside a `detail=` field (`\\\"`). Markers sit between `key="` and the value,
  so `rg 'path="/Users/x'` misses a marked line. Strip them first:
  `perl -CSD -pe 's/[\x{FDD0}-\x{FDEF}]|\\+u\{0*fd[de][0-9a-f]\}//gi' cmdr.log | rg 'path="/Users/x'`.
- **`cmdr://logs` filters the redacted lines**, so `filter=` on a raw path or name matches nothing. An agent with
  filesystem access reads the log file itself for that.

## RUST_LOG recipes

Copy-paste commands for common debugging scenarios. All include `info` as the base level.

- **Network/SMB**: `RUST_LOG=cmdr_lib::network=debug,mdns_sd=debug,smb=warn,sspi=warn,info pnpm dev`
- **Drive indexing**: `RUST_LOG=cmdr_lib::indexing=debug,info pnpm dev`
- **Indexing scanner only**: `RUST_LOG=cmdr_lib::indexing::scanner=debug,info pnpm dev`
- **Indexing FSEvents**: `RUST_LOG=cmdr_lib::indexing::watch::watcher=debug,info pnpm dev`
- **Reconcile, per subtree walk** (the detail behind the churn line below):
  `RUST_LOG=cmdr_lib::indexing::reconcile=debug,info pnpm dev`
- **Reconcile, per PATH** (the queued anchors and the dirs that couldn't be read, both Trace):
  `RUST_LOG=cmdr_lib::indexing::reconcile=trace,info pnpm dev`
- **Per-subtree churn** (needs `CMDR_CHURN_SPIKE`, see below): `RUST_LOG=cmdr_lib::indexing::churn=debug,info pnpm dev`
- **File operations (copy/move/delete)**: `RUST_LOG=cmdr_operations=debug,copy=debug,op_manager=debug,info pnpm dev`.
  The write engine is the `cmdr-operations` crate: a line with no explicit target logs under its module path
  (`cmdr_operations::transfer::…`), and many carry a short one instead (`copy`, `move`, `op_manager`,
  `conflict_resolution`, `write_durability`).
- **Operation log and undo**: `RUST_LOG=cmdr_operation_log=debug,operation_log=debug,info pnpm dev`
- **Local disk backend**: `RUST_LOG=cmdr_local=debug,local_posix=debug,info pnpm dev`
- **A transfer dialog that sits on 0 bytes, or on "Checking for conflicts..."**:
  `RUST_LOG=scan_preview=debug,conflict_scan=debug,info pnpm dev`. The scan preview logs its start, a 5 s heartbeat with
  running counts, and its outcome; the conflict check logs its start, the collision count, and which leg gave up. The
  give-up line names the leg's own share of the budget ("source stats unavailable after 9.8s of their 10.0s share"),
  which is the tell for a starved SOURCE leg: the optional source stats get a third of the 30 s, so those seconds aren't
  the destination's. Both targets are already at Debug in the log FILE, so a user's bundle answers this without a
  re-run.
- **A share that's slow, or a share browser that comes back empty**:
  `RUST_LOG=smb_fallback=debug,subprocess=debug,info pnpm dev`. `smb_fallback` says why a share is on the macOS kernel
  mount rather than a direct smb2 session (it names a rejected password as such, which is the fixable cause);
  `subprocess` says when `smbutil` / `smbclient` was stopped for not answering. Both are already at Debug in the log
  FILE, so a user's bundle answers this without a re-run.
- **Directory listing**: `RUST_LOG=cmdr_listing=debug,info pnpm dev`
- **File viewer**: `RUST_LOG=cmdr_lib::file_viewer=debug,FE:viewer=debug,info pnpm dev`
- **MTP (Android devices)**: `RUST_LOG=cmdr_lib::mtp=debug,FE:mtp=debug,info pnpm dev`
- **Volume discovery + broadcast**: `RUST_LOG=cmdr_lib::volume_broadcast=debug,cmdr_lib::volumes=debug,info pnpm dev`
- **AI/LLM**: `RUST_LOG=cmdr_lib::ai=debug,info pnpm dev`
- **Licensing**: `RUST_LOG=cmdr_lib::licensing=debug,info pnpm dev`
- **MCP server**: `RUST_LOG=cmdr_lib::mcp=debug,info pnpm dev`
- **All frontend logs**: `RUST_LOG=FE:=debug,info pnpm dev`
- **Specific FE feature**: `RUST_LOG=FE:fileExplorer=debug,info pnpm dev`
- **Everything (noisy deps suppressed)**: `RUST_LOG=debug,smb=warn,sspi=warn,mdns_sd=warn,hyper=warn pnpm dev`

Frontend log targets use `FE:{category}` where category matches the `getAppLogger('category')` name.

## RAM gauge (`CMDR_LOG_RAM_USE`)

Set `CMDR_LOG_RAM_USE=1` to prefix every log line with the backend's current memory use, right after the level:

```
2026-07-16T22:56:21.829+02:00 DEBUG (374 MB) ·smb2::client::tree  tree: fs_info done, total=...
```

```bash
CMDR_LOG_RAM_USE=1 pnpm dev
# combine with RUST_LOG to zoom in on a suspect subsystem:
CMDR_LOG_RAM_USE=1 RUST_LOG=cmdr_lib::indexing=debug,info pnpm dev
```

Use it to keep RAM at bay: the inline number turns the log into a memory timeline, so you can see which operation
coincided with a jump. It answers _when and near what_, not _what allocated_. For _what allocated_ — and before you run
`vmmap` at all, because it reports Cmdr's Rust heap under a GPU name — read
[the memory-debugging guide](memory-debugging.md). Works in dev, E2E, and prod builds. Accepts `1`/`true`/`yes`/`on`.

The number is `phys_footprint` (Activity Monitor's "Memory" metric, not RSS) of the Rust backend process only, sampled
every 100 ms. Cmdr's WebView runs in separate processes, so it's not included. Mechanism and the file-dedup caveat:
`apps/desktop/src-tauri/src/logging/DETAILS.md` § "RAM gauge".

## Churn instrumentation (`CMDR_CHURN_SPIKE`)

Set `CMDR_CHURN_SPIKE=1` to make the live FSEvents loop log per-subtree churn, rolled up the ancestor chain, on the
`indexing::churn` target at Debug. It's read-only: it writes no index state and changes no behaviour, and it costs
nothing when the variable is unset.

```bash
CMDR_CHURN_SPIKE=1 pnpm dev
```

Two knobs: `CMDR_CHURN_SPIKE_PERIOD_S` (rollup period, default `30`) and `CMDR_CHURN_SPIKE_TOP_N` (directories logged
per period, default `40`). Output is `1 + top_n` lines per period per volume.

**It only records while a live event loop runs**, so it's silent during a scan or rescan. That's the instrument being
honest, not broken. Line format, the offline analyser, and how to read a collection:
`../notes/churn-observability-spike.md`.

## The reconcile churn line

The index re-walks a subtree whenever macOS says "something under here changed" (`MustScanSubDirs`). That happens
thousands of times a day, so the per-walk lines are Debug. What reaches Info is one aggregate, at most one line per 15
minutes, and only when the window crossed a budget: more than 60 s of cumulative walk time, or more than 100,000
cumulative row changes.

```
Reconciler: heavy churn in the last 15 min: 120 subtree reconciles, 169s of walking (12s CPU), 102,229 row changes, 64+ anchors, 37 signals held back, 8,142 signals queued behind a running rescan. Top: /Users/me/Library/Caches/… (18 walks, 96s, 7s CPU), …
```

Read it as "this machine is spending real time staying in sync, and here is where". The top anchors are ranked by
accumulated walk cost, so the first one named is the folder to look at. The `CPU` figures are the walks' own thread CPU:
walking is disk-bound, so on a busy machine the walk time is mostly waiting, and a big walk time with a small CPU figure
means the CPU went somewhere else (the writer heartbeat's `writer_cpu_ms_total` is the next place to look). Each
per-walk Debug line carries its CPU the same way. `64+ anchors` means the per-window anchor list hit its cap, so the
count is a floor. `signals held back` counts the change signals the per-subtree throttle and the new-subtree settle
delay absorbed; a window that churns hard while that reads zero means one of those stopped working.
`signals queued behind a running rescan` counts the ones that arrived while the single-flight drain was already walking,
so it reads as queue pressure; it's omitted when it's zero, and it replaces what used to be a Debug line per signal.

**Silence is the expected state.** On a healthy machine the line never appears. Seeing it repeatedly is the signal
itself, not a fault in the logging.

For the per-walk detail behind it, turn the reconciler up: `RUST_LOG=cmdr_lib::indexing::reconcile=debug,info pnpm dev`.
Mechanism, budgets, and the memory bound: `crates/cmdr-index/src/indexing/reconcile/DETAILS.md`.

## Verbose logging

Toggle in **Settings > Advanced > Logging > "Verbose console output (developer)"**:

- Lifts every frontend (LogTape) category to debug, in the browser devtools console and on the bridge to the log file.
- Bumps the Rust **stdout chain** from Info to Debug (and back). The file chain stays at Debug regardless, so error
  reports are unaffected by the toggle.
- Implemented via an `AtomicU8` consulted on every record: the toggle takes effect mid-stream without rebuilding the
  logger, so no records are lost during the swap.
- `RUST_LOG` always wins at startup. The toggle takes over at runtime if the user clicks it (it overwrites the atomic
  directly).
