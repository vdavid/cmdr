# Priority — details

The transport-generic, per-volume priority mechanism for background work. Keyed by volume id; nothing SMB- or
MTP-specific lives here, so any future backend adopts it by reading the same two signals.

## Decision: signals + composed decisions, not a scheduler

**The mechanism is two process-global signals and a handful of pure decision functions — deliberately NOT a scheduler,
token broker, or queue.** Every background consumer already has a natural between-units boundary (between listings,
between images, between chunks) where it polls cheap state; a central scheduler would add ownership, fairness, and
cancellation machinery for no behavior we need. The priority order is enforced by three pairwise edges:

1. **Transfers yield to interactive** — `SmbVolume`'s foreground-yield methods
   (`crates/cmdr-smb/src/volume/foreground_yield.rs`) park `CheckpointStream` between chunks while a foreground
   operation holds a lease on the share, and for the quiet window after the last one ends. MTP answers the same question
   from its own per-device gate (a PTP session has an explicit holder, so it never needed the timestamp half).
2. **Drive indexing yields to both** — `indexing/network_scanner/scan_pace.rs` drops the walk's listing budget to ONE
   while the share is browsed OR a transfer touches it. Throttle, never stop: the budget is never zero, so forward
   progress is structural (no starvation quota to get wrong). Since the lease landed, "browsed" covers a listing's whole
   duration and not just the 2 s window after it started, so a folder that takes ten seconds to come back holds the scan
   at one listing in flight for those ten seconds. Intended, and safe by the same structural property:
   `scan_pace::tests::the_budget_is_never_zero_for_any_input` proves no clearance value the lease can produce stops the
   walk.
3. **Image enrichment yields to both** — the network pass's between-images gate
   (`media_index/network/policy.rs::volume_clear_for_enrichment`) pauses the pass (`PauseReason::NotIdle` →
   `PassOutcome::RetryWhenIdle`) while the app is foreground-busy or a transfer touches the volume, and the resume wait
   (`wait_until_idle_to_resume`) polls the SAME composed condition, so "clear enough to resume" is exactly "clear
   enough to have kept going".

## The signals

- **`foreground`** — "is the user waiting on this volume?", in two halves per volume plus an app-wide timestamp.
  - **The LEASE** is the exact half: a `ForegroundLease` guard held for the real duration of a foreground operation,
    counted per volume. The spawned directory-listing task (`file_system/listing/streaming.rs`) takes one, so a folder
    that takes ten seconds to come back reads as busy for ten seconds. Release is by DROP only, which is what makes the
    error path, a panic, and a dropped task all correct with nothing to remember.
  - **The TIMESTAMP** is the decaying half: stamped by the hot listing IPC (`commands/file_system/listing.rs`), which
    knows the volume, and refreshed whenever a lease is taken OR released. Refreshing at RELEASE is load-bearing: it is
    what starts the post-operation debounce when the operation ENDED, so a burst of arrow-key presses is one suspension
    rather than one per keystroke. The decision is the pure `is_idle(now, last, threshold)` over millis from a monotonic
    base.
  - A scoped note or lease stamp also writes the app-wide slot, so an app-wide reader never misses activity. The
    app-wide scope stays timestamp-only: a lease names a volume, and there is no app-wide claim to hold.
  - **Both halves move under one write lock**, so no reader can see the count drop before the timestamp that has to
    carry the debounce.
  - **Both halves are an EVENT too.** Every write bumps the volume's change counter (`ForegroundActivity::watch_volume`
    hands out a subscription to it), so a background user standing aside sleeps until the signal actually moves rather
    than asking again on a tick. Waiting is composed once, next to the rule:
    `cmdr_fs::volume::host::activity::wait_until_volume_free` sleeps on the lease coming back and spends ONE computed
    sleep on whatever is left of the quiet window, since nothing announces a timestamp going stale. A wake means
    "something moved", never "you are free", so it re-reads both halves every time. The SMB transfer yield
    (`crates/cmdr-smb/src/volume/foreground_yield.rs`) is its consumer, on both the source and destination arms.
  - Subscribing creates the volume's map entry, so an entry means nothing on its own: `last_millis: None` is what
    "never browsed" means.

**Composing the two is `cmdr_fs::volume::host::activity::volume_busy_for_user`, and only that.** Both background
consumers (a storage backend through `UserActivity`, the index through `AppHostPolicy::clearance`'s `volume_idle`) go
through it, so they cannot drift on what "the user is waiting on this volume" means. A consumer reading
`idle_for_volume` alone would silently lose the whole point: the operation stops counting while the user is still
waiting on it.
- **`transfers`** — a per-volume COUNT of in-flight user-initiated write operations. Fed from the one write-op
  lifecycle choke point (`write_operations::state::register_operation_status` / `unregister_operation_status`, the
  same pair that maintains the eject busy set, so the two can't drift and the finish rides the manager's panic-safe
  guard). A count, not a flag: overlapping ops keep the volume busy until the LAST ends. Deletes, trash, and drag-out
  promises count too — they all contend on the same device connection a copy does.

### How long a lease can be held

A lease lives exactly as long as the listing task that took it, and a listing cannot run forever:

- **Every SMB round trip is deadlined by the transport.** `smb2` 0.20.1 enforces a 30 s response deadline and a 20 s
  send deadline per request, and only `CHANGE_NOTIFY` is exempt (`is_long_poll`), so a `QUERY_DIRECTORY` that gets no
  answer errors out rather than hanging. A large directory is many such round trips, each individually bounded and each
  making progress; the total is bounded by the directory's size, not by a hang. (Verified by reading the `smb2` 0.20.1
  client connection module's `RESPONSE_TIMEOUT`, `SEND_TIMEOUT`, and `is_long_poll`, 2026-08-31.)
- **The user cancels.** Navigating away or pressing ESC cancels the listing's token, and the `select!` cancel arm
  returns immediately, dropping the lease while the backend unwinds behind it. That is the right answer: the pane has
  moved on, so nobody is waiting on that listing any more.
- **Even a lease that somehow persisted cannot stop background work.** The index scan's budget floor is one listing in
  flight, never zero, and an upload's destination park is hard-capped at `DEST_FOREGROUND_YIELD_HARD_CAP` (1 s) per
  park. A held lease therefore SLOWS a transfer and a scan; it can never stop either.

## The walk order (`roots.rs`)

The consumer is the index's phase machine, which covers a never-completed volume in pieces and asks
`HostPolicy::priority_roots` at every phase boundary (`indexing/lifecycle/phases/`). It is on by default
(`PHASED_FIRST_INDEX`), so this ranking is what a real first index walks first.

The ranked list is the schedule that walk follows. Everything about it follows from one
property: **it is an order and nothing else.** Such a walk covers the whole volume either way, so a root that appears,
disappears, or turns out to be a bad guess costs a few minutes of ordering and never a file that goes unindexed. That is
what makes it safe to rank on signals that are cheap, incomplete, and occasionally wrong.

**The signals, best first**, because the order IS the schedule:

1. **Last session's tab paths** from `app-status.json`, most recently active first. The strongest signal there is: it is
   literally where the user was. Read straight from the file the frontend's pane persistence writes, since the seam is a
   plain trait method with no `AppHandle` to resolve a data dir with (same `CMDR_DATA_DIR`-or-bundle-id resolution as
   `favorites/store.rs`). "Most recently active" is the focused pane's active tab, then the other pane's, then the rest:
   the closest thing the store keeps to a recency order. Only `volumeId == "root"` tabs count, and `~` expands (it is
   what a pane persists while it sits in home, so an unexpanded one would drop the most common tab there is). The
   pre-tabs scalar `leftPath` / `rightPath` keys still answer for an install nobody has touched in a while.
2. **Cmdr favorites** in the user's own order. The seed is platform-dependent (`/Applications` on macOS, the home folder
   on Linux), so the ranking takes whatever `favorites::store::list()` hands it; ❌ never assume the macOS four. Note
   that reading them seeds the defaults on a first run, exactly as the volume switcher's own read does.
3. **Where they have been working this month**, from Spotlight: the folders holding files with a
   `kMDItemLastUsedDate` inside the window, busiest first. `roots/recency.rs` decides when to ask and what to keep;
   `apps/desktop/src-tauri/src/spotlight.rs` asks. Below the two signals the user stated OUTRIGHT and above the static list, which is the whole
   of what it buys: on a true first run there are no tabs and no favorites, so without it every machine gets the same
   order. See § "The recency signal" below.
4. **The standard home folders** (`Downloads`, `Documents`, `Desktop`, `Pictures`, `Movies`, `Music`) that exist AND
   hold something. The non-empty bar is only for the folders we guessed at: a folder the user named themselves is taken
   as-is, since them saying it matters beats what happens to be in it today.
5. **Cloud roots**: every File Provider domain under `~/Library/CloudStorage`, then `~/Dropbox`, then iCloud Drive.
   After the local ones deliberately: a File Provider read can stall, and a stall must not delay `~/Downloads`. The
   domains are sorted, because `read_dir` order is arbitrary and a schedule that reshuffles between asks is one nobody
   can debug. **Known limit**: ordering only protects the WALK. Listing `~/Library/CloudStorage` and stat-ing a domain
   root are local metadata reads (macOS keeps domain roots materialized; the stalls are on unmaterialized file
   CONTENTS), but if one ever did hang, it would hang the whole answer, local roots included. If that shows up, the fix
   belongs where the seam is consulted, not here.
6. **`$HOME`**, last, sweeping up whatever the guesses missed. Last is load-bearing: first, and every later root would
   be a descendant of it and get dropped, collapsing the whole schedule into one undifferentiated walk.

**The filters** each candidate passes, in `WalkOrder::consider`:

- **`~/Library` is never a root**, though it is in scope for the index and the `$HOME` phase includes it. It is the
  biggest and churniest subtree in home (1.44M entries on David's machine, 27.7% of the whole index: Caches 423k, Mail
  395k, Application Support 210k), so walking it early spends the phase that was supposed to make the user's own files
  searchable. Its cloud children are separate candidates and stay.
- **Dedupe and descendant-drop are one test**: a path that `starts_with` an accepted root is either that root again or
  sits inside it. Paths are normalized through `components()` first, so `~/Documents` and `~/Documents/` are one root.
- **Another volume's ground is not ours to schedule** (a favorite on a share belongs to that share's index), decided by
  two guards that both run before any filesystem call. The volume manager's in-memory `mount_id_for_path` catches every
  mount Cmdr has registered, wherever it sits. The `/Volumes`- and `/Network`-style path prefixes (`/media`, `/mnt`,
  `/run/media`, `/net` off macOS) catch the rest: the registry only knows what the app registered, and registration can
  lag or never happen, so without the prefix rule a favorite on an unregistered wedged share would reach `is_dir()` and
  block the index's thread for minutes. ❌ Never a `statfs` probe (`path_is_on_network_mount`, a space query) to answer
  this: that IS the call that blocks. Residual gap, accepted: a mount at an unusual path that Cmdr hasn't registered.
- **A cap of 24**, so somebody with 200 favorites doesn't turn the first phase into a whole-drive walk.

**The TCC rule.** While the Full Disk Access decision is pending, a path `tcc_paths::is_potentially_tcc_restricted`
covers is taken on trust with no stat at all: `Path::exists()` alone raises a system popup, and several stack on top of
our onboarding modal. The protected folders exist on essentially every account, and a walk of one that doesn't simply
finds nothing. Enumerating the cloud domains is skipped entirely while pending, since that means reading a TCC-anchored
directory; a later ask picks them up. `volumes::get_favorites` follows the same rule and is the one to copy, ❌ not to
duplicate.

**Why a TTL cache rather than a snapshot at launch.** The seam's contract is "recomputed when asked", so an edited
favorites list or a new tab lands without a restart. But a caller asking once per walk boundary asks in bursts that can be
milliseconds apart, and an answer costs a couple of dozen stats plus a small file read. Ten seconds is short enough that
a change lands within a few phases and long enough that the walk never pays for the question. The lock is held across
the computation on purpose: a burst of asks then produces one answer instead of a stampede of identical stat storms.

**Only the boot volume gets an answer.** Every signal above describes one machine's layout, so a share inheriting it
would be nonsense; a share's own order is a question for whoever needs it.

### The recency signal (`roots/recency.rs`)

**The question it answers**, and the only one it answers: on a true first run there are no tabs and no favorites, so
the ranking falls back to a static list identical on every machine. This is what makes that one run personal, at the one
moment nothing else can (the index knows nothing yet, so the index's own signals can't help).

**A folder is ranked by how many recently-used files sit DIRECTLY in it**, never recursively. A recursive count would
hand `$HOME` every file below it and put it on top, which is exactly the collapse the `$HOME`-goes-last rule exists to
prevent.

**The window is 30 days.** A fresh install often follows a new machine or a migration, where the last week of activity
is unpacking rather than working, so a tighter window ranks the wrong folders on precisely the run this serves. A month
sees past that and still lets last spring's project fall off.

**Asked once per process, off-thread, and late is fine.** `HostPolicy::priority_roots` is contractually cheap (no I/O on
a contended path, no blocking lock) and a synchronous `MDQuery` plus one attribute read per result is neither. So the
first ask that finds the FDA gate settled ARMS a detached sampler thread and answers without it; later asks pick the
result up. The phase machine asks at every boundary, so it joins within a phase or two of index start. Arriving late
costs an order, never a file. The arming is a compare-and-set rather than a lock held across the spawn, because asks
arrive in bursts milliseconds apart.

**Three filters, and each is load-bearing:**

- **`~/Library` descendants are dropped here**, not by `WalkOrder::consider`, which only rejects `~/Library` ITSELF.
  A month of recency under a home directory is dominated by application-support files (measured on this machine: at a
  wide window, three of the top five folders were `Library/Application Support/Claude`, `Library/Dropbox`, and
  `Library/Keyboard Layouts`). Uncapped they would take every slot the signal has. Its `CloudStorage` and iCloud Drive
  children are the exception and stay: a file opened in Dropbox is the user's own work.
- **A folder needs at least 2 recently-used files.** One is somebody opening an attachment once; two is a place they
  went back to. ⚠️ A guess, and the first knob to turn if this ever ranks badly on a real home.
- **At most 8 folders reach the ranking.** ⚠️ Not optional: the whole ranking caps at 24 and recency outranks the
  standard home folders, so an uncapped tail of barely-used folders would push `~/Downloads` and `~/Documents` off the
  end entirely.

**Everything about it is best-effort.** Spotlight turned off, a volume that isn't indexed, a permission we don't have,
or a macOS release that changes the query language all produce an empty answer and a log line, never an error. Verified
against a live Spotlight index on macOS 26.6.2 (2026-08-27): the query matches `mdfind`'s result set exactly, and the
folder folding put `~/Downloads` on top at 13 files over a 10-year window.

**Read `kMDItemPath` off the query's own results, ❌ never `MDItemCreate` per path.** `MDQueryGetResultAtIndex` follows
the CoreFoundation GET rule, so its `MDItem` is borrowed from the query and must not be released, and every path is
copied out before the query is.

## Scope choices (why each consumer reads what it reads)

- **Enrichment: app-wide foreground.** Heavy on-device ML with no deadline; foreground work anywhere is reason enough
  to wait. Its transfer check is per-volume, though — a copy on another device says nothing about this NAS.
- **Scan pacing + transfer-yield: per-volume.** Their contention is one share's session; browsing a local folder must
  not slow a NAS scan or park a NAS copy.
- **Local enrichment reads neither.** It contends on CPU/ANE (governed by the parallelism setting, thermal backoff,
  and the memory watchdog), not on a connection; wiring it to foreground would pause local indexing for no resource
  the user is waiting on.

## Yield shapes (why not one shape)

Drive indexing throttles (budget 64 → 1) because a walk holds cheap resumable state and one listing at a time is
harmless. Enrichment pauses whole passes because a pass holds a Vision backend and prefetch buffers, and its
resume-from-store machinery already exists (staleness skips done rows). Transfers park in place between chunks because
they hold open handles a full stop would invalidate. Same signals, per-consumer shape.

## MTP status

MTP drive indexing adopts the transfer edge automatically: it paces through the same `ScanPacer`, and MTP write ops
register their volume ids in the same status cache. MTP media enrichment never background-sweeps (`media_index`
policy), so there is nothing further to wire; the interactive edge for MTP transfers stays its per-device gate (above).
