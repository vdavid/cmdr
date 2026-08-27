# Priority — details

The transport-generic, per-volume priority mechanism for background work. Keyed by volume id; nothing SMB- or
MTP-specific lives here, so any future backend adopts it by reading the same two signals.

## Decision: signals + composed decisions, not a scheduler

**The mechanism is two process-global signals and a handful of pure decision functions — deliberately NOT a scheduler,
token broker, or queue.** Every background consumer already has a natural between-units boundary (between listings,
between images, between chunks) where it polls cheap state; a central scheduler would add ownership, fairness, and
cancellation machinery for no behavior we need. The priority order is enforced by three pairwise edges:

1. **Transfers yield to interactive** — `SmbVolume`'s foreground-yield methods
   (`crates/cmdr-smb/src/volume/foreground_yield.rs`) park `CheckpointStream` between chunks while the share's
   per-volume foreground timestamp is fresh. MTP answers the same question from its own per-device gate (a PTP session
   has an explicit holder; time-based signals aren't needed there).
2. **Drive indexing yields to both** — `indexing/network_scanner/scan_pace.rs` drops the walk's listing budget to ONE
   while the share is browsed OR a transfer touches it. Throttle, never stop: the budget is never zero, so forward
   progress is structural (no starvation quota to get wrong).
3. **Image enrichment yields to both** — the network pass's between-images gate
   (`media_index/network/policy.rs::volume_clear_for_enrichment`) pauses the pass (`PauseReason::NotIdle` →
   `PassOutcome::RetryWhenIdle`) while the app is foreground-busy or a transfer touches the volume, and the resume wait
   (`wait_until_idle_to_resume`) polls the SAME composed condition, so "clear enough to resume" is exactly "clear
   enough to have kept going".

## The signals

- **`foreground`** — "when did the user last do foreground work", app-wide (one atomic) and per volume (a tiny map).
  Stamped by the hot listing IPC (`commands/file_system/listing.rs`), which knows the volume; a scoped note also stamps
  the app-wide slot so an app-wide reader never misses activity. The decision is the pure `is_idle(now, last,
  threshold)` over millis from a monotonic base.
- **`transfers`** — a per-volume COUNT of in-flight user-initiated write operations. Fed from the one write-op
  lifecycle choke point (`write_operations::state::register_operation_status` / `unregister_operation_status`, the
  same pair that maintains the eject busy set, so the two can't drift and the finish rides the manager's panic-safe
  guard). A count, not a flag: overlapping ops keep the volume busy until the LAST ends. Deletes, trash, and drag-out
  promises count too — they all contend on the same device connection a copy does.

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
