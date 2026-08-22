# Who the listing wedge could have hurt, and what our telemetry can and cannot tell us

**What this settles:** how far back the main-thread wedge described in `listing-row-fetch-quadratic-2026-08-22.md` was
reachable, which releases carried each escalation, whether it recovers on its own, and what the live feedback loops
actually say about testers hitting it. Read that note first; this one is the blast-radius companion and does not repeat
the mechanism.

⚠️ **The honest headline: we cannot tell whether a tester hit it, and we could not have.** A wedge emits nothing. The
error-report channel is opt-in and essentially unused, the crash reporter cannot see a hang by construction, and the
analytics heartbeat keeps beating from a background thread while the main thread is saturated. Every "no reports" fact
below is weak evidence, and this note labels it as such rather than reading it as an all-clear.

Every claim here is tagged **measured**, **derived**, or **guess**. § 2 carries a **release-build** benchmark taken for
this note (2026-08-22, both builds, six directory sizes); everything else quotes the investigation's and the fix's own
debug numbers with their caveats intact.

## The short answer

- **Reachable since v0.5.0** (2026-02-15), when a cursor move first cost up to 100 main-thread row lookups. **Reachable
  with no user input at all since v0.23.0** (2026-06-01), when the index started driving the same sync. **Worst from
  v0.37.0** (2026-08-03), when every row lookup got a more expensive predicate.
- **Fixed on 2026-08-22**, which is after v0.39.0 (2026-08-19). So **every released build carries it**, and the fix
  ships in the next release.
- **The tag-enrichment defect this note surfaced along the way is fixed too** (2026-08-22, after the build § 2
  measured), and it was the bigger cost at large sizes: opening a 300,000-row folder spent 225 s of CPU filling Finder
  tags and now spends 10 s. See § "What the tag fix moved".
- **So is a third one nobody had measured**: one coalesced watcher event carrying 500 removals (a directory emptied, a
  `git checkout` across a big tree) walked the listing twice per file. Every by-path lookup in the listing cache now
  goes through the same path map. See § "The rest of the by-path lookups".
- **It does not deadlock, and it does not leak.** It saturates. In principle it clears when the driver stops; in
  practice the user's way out (a keystroke, a navigation, a settings toggle) runs through the wedged main thread, so
  force quit was the reliable remedy.
- ❗ **In a RELEASE build, at the sizes people open, it was "big folders feel slow" rather than a wedge.** Measured
  2026-08-22 (§ 2): with the drive index off, a keystroke at the bottom of the listing costs 52 ms at 20,000 rows, 432
  ms at 75,000, 941 ms at 150,000, and 2,821 ms at 300,000, and the app answered every one. Never answering needs
  roughly 500,000 rows, or roughly 150,000 with the drive index on. The debug builds every other number in these two
  notes came from overstate the user's experience by about two orders of magnitude.
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
  virtualization buffer. Measured at the default 1,080 × 720 window: Full 68, Brief 121 (so 100 after the cap). ❗ The
  cost does NOT follow that ratio; see "The fan-out: Brief is not the expensive mode" below.
- **The scan depth** is the cursor's row number, from v0.9.0 onward. At the top it is near zero; at the bottom of an
  N-row directory it is N.

So bottom-versus-top for one sync is roughly N / 100: about 200× in a 20,000-row directory, about 740× in a 74,000-row
one. The index driver caps the sync rate at one per pane per 2,000 ms, and two panes can each hold one.

### What is actually measured: the release curve

**Both builds are RELEASE builds** (`pnpm build --bundles app`, universal, arm64 slice running), on `Davids-M1-MBP`
(Apple M1 Max, 10 cores, 64 GB), macOS 26.6.2 / Darwin 25.6.0, 2026-08-22. Pre-fix is `7fcaf9c5c`, the commit before
`e39f05aa8`; post-fix is `main` at `e57ef0094`. Protocol and its traps: § "Method, and what to redo".

Median of the moves the app ANSWERED, from `scripts/cursor-move-latency.py` (15 timed `move_cursor` calls per depth, in
alternating blocks). **Every cell below answered 15 of 15** inside the probe's 5 s ceiling, on both builds, at every
size. Nothing here ever stopped responding.

"Just opened" starts probing about 8 seconds after the navigation, so the post-navigation tag-enrichment pass is still
running at the larger sizes. "Settled" waits 90 to 240 seconds first, which is the steady state.

**Cursor at the bottom row**, which is the case the defect is about:

| rows    | pre, just opened | pre, settled | post, just opened | post, settled |
| ------- | ---------------- | ------------ | ----------------- | ------------- |
| 5,000   | 27 ms            | not taken    | 9 ms              | not taken     |
| 20,000  | 52 ms            | 54 ms        | 9 ms              | not taken     |
| 40,000  | 112 ms           | not taken    | 8 ms              | not taken     |
| 75,000  | 604 ms           | 432 ms       | 137 ms            | not taken     |
| 150,000 | 1,198 ms         | 941 ms       | 121 ms            | not taken     |
| 300,000 | 3,056 ms         | 2,821 ms     | 233 ms            | 14 ms         |

**Cursor at row 10**, same runs:

- **Pre-fix, settled**: 15 to 16 ms at every size from 20,000 to 300,000 rows.
- **Pre-fix, just opened**: 12 ms up to 40,000 rows, then 191 ms at 75,000, 162 ms at 150,000, and 176 ms at 300,000.
  That step is the post-navigation tag-enrichment pass, not the row lookup (see below), and it is transient.
- **Post-fix**: 3 ms in every cell, opened or settled, 5,000 rows or 300,000.

Main thread under back-to-back `move_cursor` at the bottom of the 75,000-row directory, 12 s `sample`
(`scripts/main-thread-ipc-share.py`): **pre 98.5%** of main-thread samples inside the IPC handler against **post 3.9%**;
samples with a leaf in the visibility scan (`scripts/listing-scan-leaves.py`) **3,651 against 47**; moves completed in
40 s **112 against 12,350**. So the release build saturates the main thread exactly as the debug build did, under a load
that never stops. What differs is the size of ONE command: about half a second at 75,000 rows, against a debug build
that could not finish one inside 5 s at 19,513 rows. At half a second the queue drains between keystrokes and the window
keeps answering; at five it does not.

⚠️ **Quote the table, not the `sample` runs, for what a keystroke costs.** The `sample` load sends moves as fast as the
app takes them, and the mirror's 300 ms debounce coalesces them: at row 10 of the same directory that load reports 23%
to 27% main-thread share and about 20 ms per move, while the latency probe (one move, wait for the acknowledgement, next
move, which is what a person typing does) reports 191 ms. Both are real; only the second one is a keystroke.

### The release-build thresholds (measured)

Read down the "settled, bottom" column. Full view, default 1,080 × 720 window, drive index off, one pane in the big
directory and the other in a one-file directory. Below 75,000 rows the two pre-fix columns agree (52 ms against 54 ms at
20,000), so the just-opened figure stands in where no settled run was taken.

- **Up to 40,000 rows: imperceptible.** 12 to 112 ms at the bottom, 12 ms at the top. Nobody would report this.
- **75,000 rows: noticeable.** About 0.4 s per arrow key at the bottom, and about 0.2 s anywhere in the listing for the
  first minute or two after opening it. This is the size where the pane starts feeling heavy.
- **150,000 rows: sluggish.** About 1 s per arrow key at the bottom. Navigating is unpleasant; the app still answers
  every key.
- **300,000 rows: unusable.** About 3 s per arrow key at the bottom, still answered.
- **Wedged (a keystroke that never lands) needs roughly 500,000 rows.** Derived by extrapolating the bottom-of-listing
  trend, which costs about 9.4 ms per 1,000 rows at 300,000 and is climbing (5.6 ms at 75,000, 6.2 ms at 150,000). The
  rise is **guessed** to be cache pressure: `listing/DETAILS.md` puts a 74,000-entry listing's entries at about 15 MB,
  so 300,000 rows is roughly 60 MB to walk. ❗ The threshold itself is not measured; nothing was built that large.

**The top of a listing is free once the directory has settled**: 12 to 16 ms at every size from 5,000 to 300,000 rows,
which is what the shape predicts, since the fan-out is fixed and the scan short-circuits.

### The measured debug-to-release factor, against the guessed one

This note previously guessed 10× to 30×. The comparable pair is the fix's own debug before/after at 19,513 rows against
the release run at 20,000 rows, same probe, same index-off configuration:

- **Bottom of the listing**: debug answered 0 of 15 inside 5,000 ms; release answers 52 ms. So **at least 96×**, and the
  debug side is a floor rather than a measurement.
- **Row 10**: debug 3,394 ms; release 12 ms. **About 280×.**

Two corrections fall out of that:

- The factor is **far larger than the guess**, so every debug absolute in `listing-row-fetch-quadratic-2026-08-22.md`
  overstates what a user experienced by roughly two orders of magnitude.
- The guess that a release build would leave the **shallow-depth fan-out roughly where it was**, "because that is
  dominated by IPC transport rather than by Rust code", is **refuted**: the shallow case improved more (280×) than the
  deep one (≥96×). The 68 to 100 IPC round trips per sync cost about 12 ms in release, so the transport was never the
  expensive part; the debug build's serde and command-dispatch overhead was.

### The fan-out: Brief is not the expensive mode (measured)

§ "The shape" says Brief costs materially more than Full because it saturates the 100-fetch cap while Full reports 40 to
80 rows. The fan-out part is right, and was confirmed from `cmdr://state`: Full reports a 68-row visible range at the
default window, Brief reports 121 (so 100 after the cap). The cost conclusion is wrong.

At 75,000 rows, pre-fix, just opened: **Brief 21 ms at row 10 and 683 ms at the bottom; Full 191 ms and 604 ms.** At the
bottom Brief is 13% worse, not the 47% the fan-out ratio implies. At the top Brief is **nine times better**, because the
row 10 cost at that size is the post-navigation enrichment pass rather than the fetches, and Brief does not wait on it.

### The row-10 cost is enrichment, not fan-out (measured)

❗ A 3,394 ms debug row-10 reading makes the fan-out itself look like the entry price. It isn't. In release, row 10
costs 12 ms at every size **once the directory has settled**, and the 162 to 191 ms readings at 75,000 rows and up are
transient: they disappear after 90 seconds of sitting still (191 → 16 ms at 75,000, 162 → 15 ms at 150,000, 176 → 16 ms
at 300,000), and they show up on the post-fix build too (137 ms at the bottom of 75,000 rows fresh against 14 ms settled
at 300,000 rows).

A `sample` during that window names it: `commands::file_system::listing::enrich_tags`, on a blocking-pool thread, inside
`listing::caching::apply_tags_to_listing`, whose leaf is `_platform_memcmp`. That function looked each updated path up
with a linear `entries.iter().position(...)`, under the listing cache's WRITE lock, once per update, so filling Finder
tags for an N-row directory cost O(N²) path comparisons. On the pre-fix build the main thread then blocked on the
matching read lock inside the **synchronous** `get_file_range` command (3,235 of 6,250 main-thread samples parked at one
instruction); with the listing reads `async` it costs the pane latency instead of the window. It was a **separate**
defect from the row map, which did not touch it, and it is the clearest evidence that fix (3) is what turns a wedge into
slow rows.

**It is fixed as of 2026-08-22**, on a commit after the one this note's post-fix build came from, by giving a listing a
path map beside its row map (`apps/desktop/src-tauri/src/file_system/listing/path_index.rs`, and § "Entries by path" of
that module's `DETAILS.md`). ❗ So every "post" cell in § 2 above measures a build that still carried it, which is
exactly why the just-opened column there is worse than the settled one. The rest of the cache's by-path lookups went
through the same map in the commit after; see § "The rest of the by-path lookups".

### What the tag fix moved (measured)

Same machine and protocol as § 2 (App Nap off, `caffeinate`, drive index off, one pane in the fixture and one in a
single-file directory), **arm64 release builds** of `cdc050085` against `af1b56a22`, 2026-08-22. The fixtures are fresh
(75,000 and 300,000 empty files, mean name length 33), so ❌ don't compare these absolutes against § 2's table; compare
the two columns with each other.

**Opening the directory and letting it settle**, CPU seconds the process spends between the navigation and going quiet.
With the index off this is the listing read plus the tag sweep, and nothing else touches the app during the window:

| rows    | before                         | after                        |
| ------- | ------------------------------ | ---------------------------- |
| 75,000  | 7.9 s CPU, quiet after 8 s     | 1.9 s CPU, quiet after 2 s   |
| 300,000 | 224.8 s CPU, quiet after 226 s | 10.0 s CPU, quiet after 11 s |

So opening a 300,000-row folder burned nearly four minutes of a core on tags nobody was waiting for, and now costs ten
seconds — which is roughly the 300,000 `getxattr` calls themselves (~15 µs each), the floor for reading the tags at all.

**Median cursor-move latency at the bottom row**, 15 timed moves per cell, every one answered on both builds:

| rows    | before, just opened | before, at +120 s | after, just opened | after, at +120 s |
| ------- | ------------------- | ----------------- | ------------------ | ---------------- |
| 75,000  | 66 ms               | 7 ms              | 8 ms               | 7 ms             |
| 300,000 | 42 ms               | 351 ms            | 9 ms               | 7 ms             |

⚠️ **Read the 300,000-row row carefully, and never as "just opened is the bad case".** The sweep on that build runs for
226 s, so the probe that starts 6 s after the navigation catches it before it gets going, and the one at +120 s catches
it mid-sweep. The +120 s column is the honest in-sweep number there; on the fixed build the sweep is over by then, so
its +120 s cell is a genuinely settled listing. Row 10 was 2 to 4 ms in every cell on both builds.

**Where the cost went, on a release microbenchmark of the lookup alone** (M1 Max, entries padded to the real
`FileEntry`'s size, 63-character mean path): one 500-path enrichment chunk scanned for 20 ms at 20,000 entries, 64 ms at
75,000, and 418 ms at 300,000, against a path map that builds once in 1.3 / 4.9 / 33 ms and then answers the same chunk
in 43 / 48 / 97 **µs**.

### The rest of the by-path lookups (measured as a scan count, not as latency)

The tag fix left four callers still finding a row by walking the listing: `carry_forward_tags`, `has_entry`, the removal
by path, and the position lookup inside `update_entry_sorted`. They all now go through the same map (2026-08-22,
`apps/desktop/src-tauri/src/file_system/listing/DETAILS.md` § "Entries by path").

⚠️ **Three of the four are a latent win, and this note should not pretend otherwise.** Each is one lookup per watcher
event, so no realistic call rate turns them into a visible cost, and none of them BUILDS a map (a single path is far
under the map's build-versus-scan threshold of 32). What they buy is the sweep's map: while tag enrichment is walking a
big directory, a watcher event landing in that directory is a hash rather than a walk. The reason to do it anyway is
structural: a future caller that loops any of them over a path list re-creates the quadratic two efforts just removed.

**The fourth was not latent.** `handle_directory_change_incremental` already looped one of them. It resolved each
removal's pre-removal index with its own full walk (the `directory-diff` payload needs an index in the old listing's
space) and then walked again inside the removal itself, and one coalesced watcher event carries up to 500 paths: a
directory emptied, a `git checkout` across a big tree, an unpack over a folder. **Measured** by the counting probe in
`path_index_test.rs` (2026-08-22, `cargo nextest`, 20,000-entry synthetic listing): a 500-path removal examined
**9,981,000 entries** before the fix and **20,500** after, which is one whole walk to build the map plus one lookup per
path. Scaled to the fixtures § 2 uses, the old shape is 500 walks of a 300,000-entry listing under the cache's write
lock.

❗ **No latency number is quoted for this, deliberately.** The measurement is a scan COUNT from a unit-test probe, not a
release-build timing on a running app, and manufacturing a benchmark that made a bulk delete look dramatic would
overstate what anybody experienced. The batch also still memmoves a row per removal (`Vec::remove`), which the lookup
fix doesn't touch, so a bulk delete in a huge folder is faster than it was but is not free.

### The rule of thumb, recalibrated to release

Stated in work per sync, `min(visible rows, 100) × cursor row` predicate evaluations:

- **~10⁶ evaluations** (20,000 rows with the cursor at the bottom): about 50 ms. Barely perceptible.
- **~5 × 10⁶** (75,000 rows at the bottom): about 0.4 s. Noticeable.
- **~10⁷** (150,000 rows at the bottom): about 1 s. Sluggish.
- **~2 × 10⁷** (300,000 rows at the bottom): about 3 s. Unusable, still answered.
- **~3 × 10⁷** and up: extrapolated past the 5 s acknowledgement ceiling.

The same scale on the debug builds put the wedge at ~10⁶, which is where release sits at 50 ms.

### What the drive index adds (derived, not measured)

Every number above has the index **off** on every volume, which is the minority configuration: 761 of 765 installs run
with it on (§ 4). The index does not change what one sync costs; it adds syncs with no user input, at most one per pane
per 2,000 ms cooldown. So it multiplies the measured per-sync cost by a duty cycle:

- **75,000 rows**: 0.43 s of main thread every 2 s per pane, so about 22% per pane and 43% for two panes parked deep.
  The app is busy but keeps up.
- **150,000 rows**: about 47% per pane, 94% for two. That is the edge.
- **300,000 rows**: over 100% for a single pane. It never gets ahead, with nobody touching the app.

This is a **lower bound**: an index-driven sync also runs `refresh_listing_index_sizes`, which the cursor-driven one
does not. Read against the "wedged needs ~500,000 rows" figure above, the index roughly **triples the reach of the
defect**, moving the no-user-input wedge down to around 150,000 rows. ❗ Nobody measured this. Doing so means enabling
the drive index on a whole volume, which was not something to start on an unattended shared machine; on a machine that
can afford the scan it is one run.

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

**What the evidence proves**: that a DEBUG build stayed wedged for a long time under a live index. The investigation saw
IPC time out at 7 s and found the app still wedged 15 minutes after the last user action, and the fix's probe got 0 of
15 cursor moves answered inside a 5 s ceiling. ❗ **No release build in § 2 ever reached that state**, at any size up to
300,000 rows with the index off; the escape routes below stayed open because the main thread kept draining between
keystrokes. Treat "force quit was the remedy" as a debug-build observation unless someone measures it in release with
the index on.

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

**Recommendation: a release-note line, and nothing more.** The release measurement in § 2 **strengthens** this rather
than changing it, and it changes the WORDING: a release build never stopped answering at any size a person opens, so a
note promising to fix a freeze would describe something testers did not experience. Reasons, in order of weight:

- **Nobody on a release build was wedged.** At 20,000 rows a keystroke cost 52 ms and at 75,000 rows 0.43 s. That is
  "this folder feels heavy", which is worth fixing and is not worth an apology.
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

### Draft release-note line (David's review required, ❌ nothing published)

❗ **Severity guardrail for any rewrite of these**: a release build kept answering at every size tested (§ 2), so a line
promising to fix a freeze, a hang, or an app you had to quit describes something testers did not experience. "Slow" is
the honest register.

❗ **And "big folders got much faster" now carries three fixes, not one.** The row map is the least vivid of them; the
tag sweep at 300,000 rows going from 224.8 s to 10.0 s of CPU is the number a reader will feel. **Preferred:**

> **Big folders got much faster.** Three things used to get slower the bigger a folder was, because each of them
> re-scanned the whole listing, and all three now look up what they need directly:
>
> - Opening a folder used to spend a long time filling in Finder tag dots. A folder with 300,000 files spent nearly four
>   minutes on it, and now spends 10 seconds.
> - Moving the cursor near the bottom of a folder with tens of thousands of files got slower the further down you were,
>   because finding a row meant counting up to it.
> - Changing a lot of files at once in a big folder, so the app has to catch up with what moved, used to re-scan the
>   folder once per file. It's now one pass for the whole batch.
>
> Reading a listing has also moved off the main thread, so a 100,000-file folder feels like a small one.

Second option, a single line, if the release note has no room for three bullets:

> **Big folders got much faster.** Opening one, arrowing to the bottom of one, and changing a batch of files inside one
> all used to re-scan the whole folder, over and over. They now look up what they need directly: a folder with 300,000
> files settles in 10 seconds instead of nearly four minutes, and arrow keys answer in a few milliseconds however deep
> you are.

Both follow `docs/style-guide.md`. Numbers to keep straight if David edits them: 224.8 s → 10.0 s of CPU at 300,000 rows
(§ "What the tag fix moved", measured on release builds), and a keystroke at the bottom of a 75,000-row folder at 0.43 s
before the row map (§ 2, measured on release builds). ❌ The batch-change bullet has **no** release timing behind it,
only a scan count from a unit-test probe (§ "The rest of the by-path lookups"), so don't put a number on that one.

## Method, and what to redo

- **Git archaeology**: every escalation point was confirmed by reading `git show <commit>:<path>` at that commit, not by
  trusting the commit message. Releases came from `git tag --contains <commit> --sort=creatordate | head -1`.
- **Feedback and error reports**: read straight from D1 and R2 per `docs/tooling/feedback-and-error-digest.md`, with
  `CLOUDFLARE_API_TOKEN` from the sops store. All 282 production bundles were downloaded and unpacked, and their
  manifests and logs read. Read-only throughout.
- **The config-snapshot counts depend on the persist-only-non-default rule.** If that rule ever changes, an absent key
  stops meaning "default" and every percentage in § 4 has to be recomputed.

### The § 2 release benchmark, and how to re-run it

- **The builds.** `CMDR_INSTANCE_ID=<id> APPLE_SIGNING_IDENTITY=- pnpm build --bundles app`, once per side, run to
  completion **before** any measurement. The instance id gives each build its own bundle identifier and data dir, so the
  two never collide and neither touches a real install. Expect roughly 45 minutes for the pair: `pnpm build` produces a
  UNIVERSAL binary, so cargo compiles the workspace twice (arm64, then x86_64). Signing needs `-` for ad-hoc; the real
  Developer ID is not on this machine, and the updater key is not either, so the run ends on a
  `TAURI_SIGNING_PRIVATE_KEY` error **after** the `.app` is written. That error is not a build failure for this purpose.
- **The pre-fix commit is `7fcaf9c5c`**, the commit before `e39f05aa8`. ❗ This note previously named `290a23a58~1`,
  which resolves to `e39f05aa8` and therefore already carries the row map. Building that would have measured a
  half-fixed app.
- **Fixtures**: 5,000 / 20,000 / 40,000 / 75,000 / 150,000 / 300,000 empty files under `/private/tmp` (outside
  Spotlight, and off the repo so no build can touch them), named from eight realistic patterns
  (`IMG_20240612_000123.jpg`, `invoice-2025-06-…pdf`, `libcmdr_index-<16 hex>.rlib`, and so on) for a **mean name length
  of 39 characters**. Name length is load-bearing: the predicate is two `str::contains` calls over the name, so
  one-character names would understate the per-entry cost several-fold.
- **Per run**: delete the instance's data dir, write a `settings.json` seeding `indexing.enabled: false`,
  `indexing.indexSize: false`, `onboarding.completed: true`, `developer.mcpEnabled: true`, `analytics.enabled: false`,
  and `_schemaVersion: 4`; launch the `.app` binary with `CMDR_INSTANCE_ID`, `CMDR_DATA_DIR`, and `CMDR_MCP_ENABLED=1`;
  park the **right** pane in a one-file directory (so no sibling-pane sync lands in the number, which is the ambiguity
  the debug run could not exclude); park the left pane in the fixture; then run
  `scripts/cursor-move-latency.py <shallow> <deep> 3`.
- ⚠️ **Disable App Nap, or you will measure macOS instead of Cmdr.**
  `defaults write com.veszelovszki.cmdr-<id> NSAppSleepDisabled -bool YES`, plus a `caffeinate -dimsu` for the session.
  Without it, a run that let the app sit idle for 90 seconds before probing read **1,158 ms** at the bottom of a
  20,000-row directory; with it, the same protocol read **54 ms**. The artifact is 20×, it is build-independent (the
  post-fix build showed 70 ms against 9 ms), and it invented a complete false finding: a "pre-fix release builds wedge
  with no user input at 150,000 rows, 0 of 15 moves answered" that evaporated entirely once App Nap was off (15 ms).
  This box runs lid-shut and headless, which is the worst case for it.
- ⚠️ **`scripts/main-thread-ipc-share.py` reported 0.0% on the first release sample.** A debug build carries an IPC
  command through wry's custom URL scheme; a release build carries it through the WebKit script-message handler, so the
  script's single entry-frame marker never matched and a 98.5%-busy main thread read as free. Fixed in `0ff4890a6`: both
  markers are matched, and a sample with neither is now an error rather than a 0%.
- ⚠️ **`scripts/listing-scan-leaves.py` counts leaves process-wide, not on the main thread.** Its top-of-stack numbers
  are a good before/after signal and are NOT evidence about which thread paid; read the tree for that. Following its
  histogram named `apply_tags_to_listing` as a main-thread cost here when the function actually runs on the blocking
  pool.
- ⚠️ **Two drive patterns give two different answers, and only one is a keystroke.** Sending `move_cursor` back to back
  as fast as the app takes them lets the mirror's 300 ms debounce coalesce the syncs, so it reports about 20 ms per move
  at row 10 of a 75,000-row directory where the latency probe reports 191 ms. Use the latency probe for anything
  described as "what a keystroke costs".
- **Still not measured**: the drive index ON at a large size (§ 2 derives it), and any size past 300,000 rows.
