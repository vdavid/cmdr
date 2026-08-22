# Who the listing wedge could have hurt, and what our telemetry can and cannot tell us

**What this settles:** how far back the main-thread wedge described in `listing-row-fetch-quadratic-2026-08-22.md` was
reachable, which releases carried each escalation, whether it recovers on its own, and what the live feedback loops
actually say about testers hitting it. Read that note first; this one is the blast-radius companion and does not repeat
the mechanism.

⚠️ **The honest headline: we cannot tell whether a tester hit it, and we could not have.** A wedge emits nothing. The
error-report channel is opt-in and essentially unused, the crash reporter cannot see a hang by construction, and the
analytics heartbeat keeps beating from a background thread while the main thread is saturated. Every "no reports" fact
below is weak evidence, and this note labels it as such rather than reading it as an all-clear.

Every claim here is tagged **measured**, **derived**, or **guess**. Nothing was benchmarked for this note; the
performance numbers are the ones the investigation and the fix took, quoted with their own caveats intact.

## The short answer

- **Reachable since v0.5.0** (2026-02-15), when a cursor move first cost up to 100 main-thread row lookups. **Reachable
  with no user input at all since v0.23.0** (2026-06-01), when the index started driving the same sync. **Worst from
  v0.37.0** (2026-08-03), when every row lookup got a more expensive predicate.
- **Fixed on 2026-08-22**, which is after v0.39.0 (2026-08-19). So **every released build carries it**, and the fix
  ships in the next release.
- **It does not deadlock, and it does not leak.** It saturates. In principle it clears when the driver stops; in
  practice the user's way out (a keystroke, a navigation, a settings toggle) runs through the wedged main thread, so
  force quit was the reliable remedy.
- **Nothing in the wild reports it, and nothing would have.** Five in-app feedback messages exist in total, none about a
  freeze. Of 282 production error bundles, 141 of the 144 that carry an install id come from one install (David's, by
  the worktree paths in their notes). Since 2026-08-03, **one install out of 765 has auto error reporting switched on**.

## 1. When it became reachable

Four escalation points, each verified by reading the code at that commit rather than the commit message.

**v0.3.0 (2026-01-14), `f6dcf2738` 2026-01-11: the mirror exists but cannot wedge.** `syncPaneStateToMcp` fetched a
fixed window of rows 0 through 99 and ran once per directory load. `get_file_at` already collected every visible entry
into a `Vec` per call when hidden files were off, so one navigation cost 100 full scans, but nothing re-fired it.
(Measured: code at that commit.)

**v0.5.0 (2026-02-15), `1061fad78` 2026-02-05, the MCP rewrite: this is when a cursor move started costing 100
main-thread scans.** Two changes land together. The fetched window follows the pane's **visible range** instead of the
first 100 rows, and the sync gains eight callers, including `setCursorIndex`, `handleVisibleRangeChange` (every scroll
frame), and every selection change. There is no debounce yet. Because `get_file_at` still collected the whole visible
`Vec` on every call, the cost was **the same at the top of the listing as at the bottom**: 100 × N predicate evaluations
per sync, on the main thread, for an N-row directory. (Measured: code at that commit.)

**v0.6.0 (2026-03-09), `e6f268c3d` 2026-02-24: the 300 ms debounce arrives.** Titled "Improve arrow up/down performance
in big folders", it wraps the sync in `createDebounce(…, 300)`. It cuts the rate, not the per-sync cost. (Measured: the
diff.)

**v0.9.0 (2026-03-23), `33ec2f279`: the cost becomes depth-shaped.** `get_file_at` switches to
`visible_entries(…).nth(index)`, so the top of a listing short-circuits and gets close to free while the bottom keeps
paying. This is the shape the investigation profiled. It is an improvement for most users and a wash for anyone parked
deep. (Measured: the diff.)

**v0.23.0 (2026-06-01): the index becomes a driver, so the app can wedge with nobody touching it.** Two commits in the
same release:

- `66712c2d2` (2026-05-24), progressive reveal of folder sizes: `run_background_verification` now emits one
  `index-dir-updated` **per newly-scanned subtree** instead of one at the end, and `hasDescendantUpdate` stops dropping
  the `/` full-refresh sentinel and the pane's own directory. Both halves raise the event rate the pane reacts to.
- `f37401520` (2026-05-29), size-pending status via MCP: `refreshIndexSizes()` gains a `debouncedSyncMcp.call()`, so an
  index event now reaches `buildMcpFileList` at all.

Before this release the index storm could not reach the row fetches. After it, a pane parked in a big directory pays a
sync per 2,000 ms cooldown window for as long as the index keeps emitting, with no user input. (Measured: both diffs.)

**v0.37.0 (2026-08-03), `66e60c3b2` 2026-08-01: every examined row gets a more expensive predicate.** The staging-temp
filter moves onto the shared read path, so `visible_entries` wraps **both** branches in a `Filter` and every entry on
the way to `index` runs `is_hidden_from_listings`, which is the profile's leaf frame (`is_staging_temp_name`). Before
this, hidden-files-off ran a single `starts_with('.')` per entry, and hidden-files-on ran no predicate at all
(`Box::new(entries.iter())`, whose `nth` is O(1)). So this release both raised the constant factor on the common path
and turned the hidden-files-on path from O(1) into O(index). (Measured: the diff. The size of the constant-factor
increase is **not** measured.)

**Never gated on MCP.** The only gate is `syncsToMcp`, a pane-kind capability that is `true` for `local`, `smb`, `mtp`,
and `archive` panes and `false` only for `network` and `search-results`
(`apps/desktop/src/lib/file-explorer/pane/volume-capabilities.ts`). No version ever checked `developer.mcpEnabled` or
whether a client was subscribed; `git grep mcpEnabled` over the whole `file-explorer` tree at `v0.38.1` returns nothing.
An SMB share or a phone over MTP is as exposed as a local disk. (Measured: code at `v0.38.1`.)

## 2. How bad, at what size

### The shape (derived)

Work per sync is `min(visible rows, 100)` row fetches, each one a separate IPC round trip onto the main thread, each one
scanning roughly `cursor row` entries. Two independent multipliers:

- **The fan-out** is capped at 100 and is set by how many rows the view reports as visible. **Brief mode saturates the
  cap**: its range is `columns × itemsPerColumn`, easily past 100. Full mode reports the rows on screen plus the
  virtualization buffer, so roughly 40 to 80. So the same directory costs materially more in Brief than in Full.
- **The scan depth** is the cursor's row number, from v0.9.0 onward. At the top it is near zero; at the bottom of an
  N-row directory it is N.

So bottom-versus-top for one sync is roughly N / 100: about 200× in a 20,000-row directory, about 740× in a 74,000-row
one. The index driver caps the sync rate at one per pane per 2,000 ms, and two panes can each hold one.

### What is actually measured

⚠️ **Every performance number that exists for this defect came from a debug build.** There is **no release-build
measurement at all**, and Rust debug builds are typically an order of magnitude or worse on a tight predicate loop like
this one. Treat the absolutes as debug-only and the ratios as the transferable part, exactly as
`listing-row-fetch-quadratic-2026-08-22.md` instructs.

From that note and the fix's before/after (dev/debug builds, macOS 26.5.2 / Darwin 25.6.0, 2026-08-22):

- **19,513 rows, index off on every volume, cursor at row 19,100**: 0 of 15 `move_cursor` probes answered within the 5 s
  ceiling. Fully wedged.
- **19,513 rows, index off, cursor at row 10**: 1 of 15 answered, at 3,394 ms. Also unusable, at the **top** of the
  listing.
- **74,144 rows, index at its ordinary resting tick, cursor at bottom**: main thread 81% to 98% of samples inside the
  IPC handler; `webview_execute_js` and keyboard IPC time out at 7 s.
- **74,144 rows, cursor at top**: 13% of a core on the branch build, 6% on `main`. Usable.
- After the fix, both depths answer 15 of 15, at a 6 ms and 23 ms median.

### The row-10 result is the important one (derived)

At row 10 the scans are trivial: about 100 fetches over rows 0 to 100 is roughly 5,000 predicate evaluations, which
cannot cost 3.4 seconds on any build. So at shallow depth **the fan-out itself is the cost**, not the scan: 100
sequential IPC round trips, each hopping through wry's URL-scheme handler onto the main thread, each with its own serde
round trip. That matches the fix's own decomposition, where the shallow case dropped to 6 ms once one `getFileRange`
replaced 100 `getFileAt` calls.

This changes the mental model in a way worth carrying forward: **cursor depth is the multiplier, but it is not the entry
price.** A big directory in Brief mode was expensive at any cursor position.

⚠️ One alternative the data cannot exclude: the probe ran with two panes, and the other pane's cursor position was not
recorded, so some of the row-10 cost may have been the sibling pane's deep sync. Nobody measured it either way.

### The rule of thumb (derived, and soft)

Stated in work per sync, which is the part that transfers across builds:

- **Under ~10⁴ predicate evaluations per sync** (a few thousand rows, cursor anywhere, or any directory with the cursor
  near the top in Full mode): the scan is not the cost. Whatever remains is the 40 to 100 IPC round trips.
- **Around 10⁶** (roughly 10,000 rows with the cursor at the bottom, or 100,000 rows a tenth of the way down): the
  measured debug builds were already wedged.
- **Around 10⁷** (the 74,144-row bottom-of-listing case): the main thread spent 81% to 98% of its time in the IPC
  handler and IPC timed out at 7 s.

❌ **Do not quote a release-build row count from this note.** The one datum that would pin it does not exist. A guess,
labelled as one: a release build probably buys somewhere between 10× and 30× on this loop, which would move the wedge
threshold from the measured ~19,500 rows into the low hundreds of thousands, while leaving the shallow-depth fan-out
cost roughly where it is because that is dominated by IPC transport rather than by Rust code. **Nobody measured this.**

**What would settle it in about 20 minutes**: build the pre-fix commit (`290a23a58~1`) in release, park a pane at the
bottom of directories of 5k, 20k, and 75k rows with the index off, and run `scripts/cursor-move-latency.py` at each.
That gives the release-build thresholds directly and against the same probe the fix used.

## 3. Does it recover on its own?

**Mechanically, yes; from the user's seat, no.** (Derived from the code, anchored on the investigation's observations.)

It is saturation, not deadlock. No lock is held across the work, the sync queue does not grow without bound
(`throttledRefresh` **drops** events during its 2,000 ms cooldown rather than queueing them), and nothing leaks. Stop
the drivers and the main thread frees up on the next window.

But the drivers do not stop on their own while a pane sits in a big directory on an indexed volume:

- The index emits `index-dir-updated` at its ordinary resting rate, roughly once a second on an indexed volume, which is
  more than enough to re-arm both panes every 2,000 ms.
- Each of the two panes can hold its own cooldown, so the ceiling is about one sync per second across the app. If one
  sync costs more than a second of main-thread time, the app never gets ahead.

And the three ways a person would escape all run through the wedged main thread: pressing a key to navigate out,
clicking a breadcrumb, and opening Settings to switch off the index. That is why the practical answer was force quit.

**What the evidence proves**: that it stayed wedged for a long time under a live index. The investigation saw IPC time
out at 7 s and found the app still wedged 15 minutes after the last user action, and the fix's probe got 0 of 15 cursor
moves answered inside a 5 s ceiling.

**What it does not prove**: that the wedge is permanent. Nobody parked a pane in a large, quiet directory on a fully
settled index and waited. The 15-minute observation was taken in `target/debug/deps`, a directory that was itself
churning, which keeps the index busy. **Guess**: on a settled index the app comes back once the events stop, so the
worst realistic case is "wedged for as long as the index has work", which for a first scan of a home folder is minutes,
not seconds.

**One consolation that is real**: since the fix, the listing read commands are `async`, so even a future O(n) read
degrades into slow rows rather than an unanswerable window. The failure mode that made this severe is closed
structurally, not only for this one accessor.

## 4. Did testers hit it? What the feedback loops say

### The population that could have (measured)

From the `heartbeat` table in the `cmdr-telemetry` D1 database, release builds only, read 2026-08-22. `anal_id` is a
per-install analytics id, so these are installs and not people; a reinstall mints a new one.

- Distinct installs by month: **350 in 2026-06** (2,237 beats), **915 in 2026-07** (4,648), **788 in 2026-08 through the
  22nd** (6,352). The earliest heartbeat row is 2026-06-10, so anything before that is invisible here.
- Distinct installs per version, with v0.37.0 onward being the worst window: **v0.37.0 291**, **v0.38.0 68**, **v0.38.1
  362**, **v0.39.0 71**.
- **Engagement since 2026-08-03** (765 installs): **691 heartbeat on exactly one day**, 32 on two or three days, 15 on
  four to seven, and **27 on eight or more**. So the population of people who use Cmdr regularly in the worst window is
  around 74, of whom about 27 are heavy users.

⚠️ Two readings of that one-day bucket, and the data cannot choose between them: someone tried the app and moved on, or
someone hit something bad and never came back. A wedge ending in a force quit looks exactly like the first.

⚠️ `AGENTS.md` describes the beta as "a few dozen early-stage-aware users". The heartbeat table says hundreds. Worth
reconciling, though not in this note.

### The amplifier is on almost everywhere (measured)

The heartbeat carries a PII-free config snapshot, and the frontend persists only non-default values, so an absent key
means the default.

- `indexing.enabled` defaults to `true`. Of the 765 installs since 2026-08-03, **4 persisted `false`**. So roughly 761
  run with the drive index on, which is the driver that makes the wedge reachable without user input.
- `developer.mcpEnabled` is explicitly `true` on **2** installs, which confirms from the other direction that this was
  never an MCP-users-only path.

### Nothing in the wild reports it (measured, and weak)

- **In-app feedback**: the `feedback` table holds **five rows in total**, from 2026-06-12 to 2026-08-19. They are about
  column widths, the Copy Path shortcut, the delete key and exFAT confirmations, Ask Cmdr not responding with an OpenAI
  key, an Anthropic key being rejected, and Downloads opening twice. **None mentions a freeze, a hang, a beachball, or
  an unresponsive window.**
- **Error reports**: the `cmdr-error-reports` R2 bucket holds **282 production bundles** from 2026-05-12 to 2026-08-21
  (90-day lifecycle, so nothing older survives). 272 are auto-sends and 10 are user-initiated. Of the 144 that carry a
  `diagId`, **141 belong to a single install**, whose auto-notes name David's own worktree paths; the other three come
  from two installs. The remaining 138 predate the `diagId` field. The dominant clusters are an updater failure and a
  volume identity conflict, both from that one install.
- **The 10 user-initiated bundles** carry their author's note verbatim. They are about drive indexing after a
  disconnect, a failed eject, local network access on macOS 27, copying empty folders, and two pieces of praise. **None
  is about the app freezing.**
- **The pre-fix hot path itself is silent by construction**: `get_file_at`'s out-of-bounds branch logs at `debug`, not
  `error`, and the error level is the auto-report threshold (`apps/desktop/src-tauri/src/error_reporter/CLAUDE.md`).
  Grepping every bundle's logs finds `get_file_at` lines in nine of them, all on listings of one to eight entries.
  Nothing large.

### Why the silence proves close to nothing (measured plus derived)

Four independent reasons a wedge leaves no trace:

- **Auto error reporting is off for effectively everyone.** `updates.errorReports` defaults to `false`. Of the 765
  installs since 2026-08-03, **one** has it persisted as `true`. Crash reports are on for three. So even an error that
  did fire would reach us from a population of about one.
- **A hang is not an error.** Nothing in the hot path logs above `debug`, so there is no error to fire against even for
  that one install.
- **A hang is not a crash.** The crash reporter handles `SIGSEGV`, `SIGBUS`, `SIGABRT`, and Rust panics
  (`crash_reporter/mod.rs`). Force Quit sends `SIGKILL`, which cannot be caught. **A wedge that ends in a force quit
  produces nothing, by construction.**
- **There is no hang detector.** `rg` over `apps/desktop` and `crates` finds no main-thread stall watchdog. The two
  watchdogs that exist are for something else: the index memory watchdog, and `$lib/focus-watchdog.ts`, which warns when
  keyboard focus has no home.
- **Analytics would have shown a healthy install.** The `/heartbeat` loop is a background Rust task, so it keeps
  reporting hourly while the main thread is saturated. Frontend events ride the `track_event` IPC and would stop, so the
  in-principle signature is "heartbeats present, frontend events absent". Nobody looks for that, and `pane_navigated`
  carries only `volume_kind`, with **no directory-size bucket**, so PostHog cannot even say how often testers open large
  directories.

**Blocked, and what would unblock it**: PostHog is the one loop not readable from this machine. A PostHog personal API
key (or a look at the PostHog UI) would allow the one query this note could not run: for installs on v0.37.0 through
v0.39.0, are there sessions with an `app_launched` and hourly heartbeats but no `pane_navigated` for a long stretch?
That is a weak signal and would not be conclusive, but it is the only wild-data probe that exists. Everything else here
was answered with `CLOUDFLARE_API_TOKEN`, which this machine has.

## 5. What to tell testers

**Recommendation: a release-note line, and nothing more.** Reasons, in order of weight:

- We have no evidence any specific person hit it, and no way to identify one if they did. A direct heads-up would have
  no addressee.
- The trigger needs a large directory plus time parked in it, which is not a common beta session.
- It is fixed and ships in the next release, so the useful action for anyone who did hit it is "update", which is what a
  release note says.
- Feedback carries no reply-to address on any of the five rows, so there is nobody to write back to even if we wanted
  to.

**Guess, flagged as one**: if anyone did hit this, they are in the 691 one-day installs since 2026-08-03 and are already
gone. Nothing recovers them, which is another argument for spending the effort on the release note rather than on
outreach.

### Draft release-note line (David's review required, not published)

> **Big folders stay responsive.** Sitting deep in a folder with tens of thousands of files could make Cmdr stop
> answering the keyboard while the drive index worked in the background. Row lookups now go straight to the row instead
> of counting up to it, and the listing reads have moved off the main thread, so the window keeps responding however
> large the folder is.

Second option, if David would rather name the severity plainly:

> **Fixed: Cmdr could stop responding in very large folders.** With the cursor deep in a folder holding tens of
> thousands of files, background indexing could tie up the app until you quit it. That is fixed, and the listing reads
> that caused it can no longer hold up the window.

Both follow `docs/style-guide.md`. The second is the more honest one and the one to prefer if David is comfortable
naming a force quit in a release note.

## Method, and what to redo

- **Git archaeology**: every escalation point was confirmed by reading `git show <commit>:<path>` at that commit, not by
  trusting the commit message. Releases came from `git tag --contains <commit> --sort=creatordate | head -1`.
- **Feedback and error reports**: read straight from D1 and R2 per `docs/tooling/feedback-and-error-digest.md`, with
  `CLOUDFLARE_API_TOKEN` from the sops store. All 282 production bundles were downloaded and unpacked, and their
  manifests and logs read. Read-only throughout.
- **The config-snapshot counts depend on the persist-only-non-default rule.** If that rule ever changes, an absent key
  stops meaning "default" and every percentage in § 4 has to be recomputed.
- **The one measurement worth taking**: the release-build thresholds in § 2, which would replace this note's softest
  section with numbers.
