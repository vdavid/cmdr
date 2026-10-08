# Handing mimalloc's post-burst slack back, and the system allocator's settle (2026-09-27)

**What this settles:** whether a forced `mi_collect(true)` (or, under the system allocator,
`malloc_zone_pressure_relief(NULL, 0)`) returns the ~240 MiB of slack that `main` keeps 15 minutes after a burst
(`search-arena-reload-2026-09-27.md` § "Still open"), and where that slack sits. **Neither call returns anything, and no
contained fix under mimalloc turned up.** The slack is dirty memory inside page spans that still exist: empty pages that
idle threads keep (30–95 MiB) and sparse pages a few live blocks pin (~120 MiB), plus 20–50 MiB of unpurged free slices.
Collecting on each tokio worker as it parks didn't move it either. Under the system allocator the same bursts settle to
a median 241 MiB at +15 min against mimalloc's 403, at the price of a higher burst peak. Raw numbers:
`allocator-slack-release-2026-09-27.csv` (one row per round, condition, and sample point; MiB) and the summary at the
end.

## Method

- **Binary**: release build of `ebe34f89c` (`pnpm tauri build --no-bundle --target aarch64-apple-darwin`) plus an
  experiment-only `main.rs`: the `CMDR_EXP_ALLOC` runtime allocator switch from `allocator-comparison-2026-09-23.md`,
  and a thread that watches a trigger file and runs `mi_collect(true)`, `malloc_zone_pressure_relief(NULL, 0)`, or
  `mi_debug_show_arenas()` followed by a `mach_vm_page_range_query` residency map of every arena (per 64 KiB slice, how
  many 16 KiB pages count toward the footprint).
- **Conditions, launched together each round**: A mimalloc; B mimalloc plus `mi_collect(true)` 45 s after the last burst
  (the arena's idle drop happens at ~+30 s); C system; D system plus pressure relief at +45 s. At +15 min, A gets a
  collect and C a relief too, and every instance is censused again at +16.
- **Data and protocol**: the arena-reload note's. `sqlite3 .backup` of prod's DBs plus clones of the small state,
  secrets, MCP files, crash report, and lock excluded; analytics, update checks, reports, the global shortcut, media
  indexing, network discovery, and `network.directSmbConnection` off; `lsof` showed no prod file and no port-445 socket.
  Idle 8 min; three bursts (MCP search `*.pdf`, then the 200,000- and 100,000-entry folders); census at +1, +5, +15 min;
  peak from a 50 ms `proc_pid_rusage` poll.

## Round 1 (load average 12–43)

MiB footprint, pre → peak → +1 / +5 / +15 / +16 min:

- A mimalloc: 300 → 1,457 → 436 / 466 / 417 / 411 (collect at +15).
- B mimalloc + collect: 311 → 1,376 → 451 / 469 / 456 / 453.
- C system: 222 → 1,780 → 183 / 223 / 241 / 272 (relief at +15).
- D system + relief: 216 → 1,797 → 180 / 217 / 227 / 230.

- **`mi_collect(true)` hands nothing back**: 43–77 µs, footprint −1 MiB, at +45 s and at +15 min alike.
- **Pressure relief releases 0 bytes**, at +45 s and at +15 min: by then the system allocator had already returned the
  arena on its own. This round the system allocator had no post-burst spike: 180 MiB at +1 min.
- **Where mimalloc's slack sits**: at +45 s, one 1 GiB arena held ~375 MiB resident, spread over its first ~29 chunks of
  32 MiB. `mi_debug_show_arenas` elides most of them (it stops at a heuristic "last used slice"), so it can't say which
  of those slices are pages and which are free; a page walk answers that (below).

## Round 2 (load average 4–47; the instances idled 48 min instead of 8, a pause mid-run)

MiB footprint, pre → peak → +1 / +5 / +15 / +16 min:

- A mimalloc: 272 → 1,316 → 357 / 367 / 432 / 437 (collect at +15).
- B mimalloc + collect: 307 → 1,454 → 455 / 430 / 426 / 428.
- C system: 161 → 1,729 → 794 / 172 / 382 / 382 (relief at +15).
- D system + relief: 159 → 2,294 → 1,480 / 175 / 282 / 282.

- **The system allocator's post-burst spike is real but short**: 794 and 1,480 MiB at +1 min, down to 172–175 by +5.
  Pressure relief 25 s after the arena drop released **0 bytes** and D still read 1,480 at +1 min, so relief doesn't
  shorten it; macOS returns that memory on its own schedule. Its +15 min reading isn't always low either: C rose to 382
  (328 MiB dirty in the malloc zones) with nothing running.
- **`mi_collect(true)` again ~nothing**: −4 MiB at +45 s, −9 at +15 min.
- **The page walk places mimalloc's slack inside pages, not in free slices.** A `mi_heap_visit_blocks` walk mapped every
  page's span onto the arena and split the residency map (MiB):
  - A before the bursts: pages span 252, live 91, resident in page spans 173, resident outside any page 26.
  - A at +45 s: pages 347, live 112, resident in pages 265, outside 46. B (after its collect): pages 421, live 114,
    resident in pages 349, outside 48.
  - A at +15 min: pages 435, live 163, resident in pages 335, outside 40; after the collect 423 / 162 / 324 / 42.
  - So ~150–190 MiB of the slack is dirty memory inside the spans of pages that still exist: free blocks, or memory past
    a page's initialized capacity that an earlier page on the same slices dirtied. Free slices hold only 40–50 MiB. A
    collect from a helper thread can't touch it: `mi_collect` collects the calling thread's pages plus arena purges, and
    these pages belong to other threads.

## Round 3 (load average 5–28)

MiB footprint, pre → peak → +1 / +5 / +15 / +16 min:

- A mimalloc: 276 → 1,353 → 419 / 436 / 348 / 348 (collect at +15).
- B mimalloc + collect: 305 → 1,463 → 452 / 458 / 380 / 380.
- C system: 186 → 1,983 → 534 / 241 / 232 / 235 (relief at +15).
- D system + relief: 177 → 1,977 → 168 / 243 / 211 / 212.

- Collect and relief: nothing again (−0.3 MiB and 0 bytes).
- **The in-page slack splits into empty pages and sparse ones.** The walk now also sums residency in pages with no live
  block (MiB):
  - A at +45 s: pages 395, live 113, resident in pages 299 (in empty pages **82**), outside pages 29.
  - B at +45 s, before and after its collect: in empty pages 96 → 91.
  - A at +15 min: pages 309, live 111, resident in pages 231 (in empty pages **41**), outside 44.
  - Empty pages are what their owning thread frees when IT collects (mimalloc v3 keeps a retired page per size class per
    thread until that thread allocates again, and a page freed into from other threads isn't counted empty until the
    owner collects). The rest (~120 MiB) is sparse pages: a few live blocks holding a 64 KiB–4 MiB page.

## Round 4: collecting on every tokio worker as it parks (load average 5–13)

Tauri's runtime built in `main.rs` (`tauri::async_runtime::set`) with an `on_thread_park` hook that runs
`mi_collect(false)` on the parking worker. E collects at most once a second per thread, G on every park. A mimalloc and
C system ran beside them. MiB footprint, pre → peak → +1 / +5 / +15 min:

- A mimalloc: 204 → 1,459 → 289 / 292 / 276.
- E park collect, 1 s: 204 → 1,672 → 279 / 274 / 276.
- G park collect, every park: 246 → 1,468 → 363 / 365 / 367.
- C system: 140 → 2,657 → 362 / 363 / 369 (293 MiB of it swapped in the malloc zones).

- **Park collects don't help.** E ends where A does. G ends 90 MiB higher: its page walk shows 95–98 MiB resident in
  free slices against A's 18–22, so pages it frees go back to the arena and stay dirty there (the purge claims a range
  only if nothing re-took it first, and a collect on every park keeps re-taking slices). Empty pages at +15 min: A 39, E
  37, G 30, so the owner-thread collect recovered at most ~10 MiB.
- **They cost CPU during bursts**: E spent 438 ms and G 900 ms inside `mi_collect` over the round, nearly all of it
  during the bursts (an average collect is 4–15 µs at idle). Idle CPU over a 290 s window: A 0.82%, E 0.86%, G 0.92%, C
  0.83% of a core; within noise.
- This round's machine was quiet, and mimalloc settled under 300 MiB for the first time; the system allocator had its
  worst round.

## Summary across the four rounds

MiB footprint, median [min–max]. Mimalloc is A in every round plus B (rounds 1–3) and E (round 4); system is C in every
round plus D (rounds 1–3). G is left out.

- **Burst peak**: mimalloc 1,458 [1,316–1,672]; system 1,976 [1,729–2,657]. The system allocator costs ~500 MiB more at
  the peak.
- **+1 min**: mimalloc 436 [279–455]; system 362 [168–1,480]. Two of seven system runs still held 794 and 1,480 a minute
  after the bursts; the rest had already returned to 168–362.
- **+5 min**: mimalloc 436 [274–469]; system 223 [172–363].
- **+15 min**: mimalloc 403 [276–456]; system 241 [211–382]. Paired per round (C − A): −176, −50, −116, +93.
- **Under 300 MiB at +15 min**: mimalloc 2 of 8 runs, system 5 of 7.
- `mi_collect(true)` from a helper thread: −0.3 to −9 MiB in six calls, 43 µs–1.2 ms each. Pressure relief: 0 bytes in
  six calls.

## What this means

- **There's no allocator call that hands the slack back.** A forced collect only frees the calling thread's pages and
  purges slices already scheduled; the slack is in other threads' pages. Pressure relief finds nothing because macOS
  malloc returns the freed arena on its own schedule, which is what the 1–2 minute transient is.
- **The mimalloc slack is fragmentation more than retention.** Empty pages held by idle threads are the smaller share
  and shrink by +15 min on their own (82 → 41 MiB in round 3). The larger share is sparse pages: a listing or search
  allocates millions of small blocks interleaved with longer-lived ones, and when the burst's data goes, the long-lived
  blocks pin their pages. The source-side fix that would address it is to give the burst's big transient structures
  (listing entries, the search arena's side tables) their own allocation region and drop it wholesale, which is a design
  change, not a contained one. Not attempted here.
- **The system allocator meets the settled target most of the time** (median 241, 5 of 7 runs under 300 MiB), and search
  speed no longer separates the two (`search-loop-allocations-2026-09-27.md`). What it costs: ~500 MiB more at the burst
  peak, a transient that sometimes lasts past a minute, an occasional bad settle (382 and 369, mostly swapped
  malloc-zone memory), and glibc on Linux, which is unmeasured. What switching takes:
  `allocator-comparison-2026-09-23.md` § "What switching would take".

## Check round after the switch (2026-09-30)

After this note, macOS moved to the system allocator (`crates/cmdr-fs/DETAILS.md` § "Which global allocator"). One round
of the same protocol checked the switch commit on a release build
(`pnpm tauri build --no-bundle --target aarch64-apple-darwin`), with a `--features mimalloc` build of the same commit
launched beside it: C system, A mimalloc. Data was a snapshot of prod 0.48.0 taken by holding a read transaction and
`cp -c`-ing each DB with its `-wal` (the hub's isolation rule says why not `.backup`). Load average 5–99, from sibling
agents' builds: both instances' index writers worked through 2.2–3.4 M FS-event messages over the round.

MiB footprint, idle 8 min → peak → +1 / +5 / +10 / +15 min:

- C system: 206 → 1,825 → 777 / 403 / 389 / 308.
- A mimalloc: 306 → 1,582 → 476 / 513 / 514 / 517.

- **The settle lands in the system allocator's range above** (+15 min: 308 against 211–382), and 209 MiB under the
  mimalloc build beside it. The peak (1,825) and the +1 min transient (777) are inside this note's system ranges too.
- **At +15 min**, C's default zone held 178 MiB live in 583 reserved, and malloc's slack across every zone was 102 MiB;
  A's census read 180 live and 267 slack.
- **Idle CPU isn't worse.** With indexing off (MCP on), one 240 s window side by side: C 0.229% of a core, A 0.241%;
  footprint 120 against 153 MiB. Under the round's indexing churn, C spent 10.5–11.5 s of CPU per 100,000 writer
  messages in the two post-burst windows, A 12.1–17.8.

## Harness

The experiment `main.rs` isn't on `main`. Its pieces, for a rerun: the `CMDR_EXP_ALLOC` switch; a trigger-file thread
that runs `mi_collect(true)`, `malloc_zone_pressure_relief(NULL, 0)`, or the residency map; the residency map itself
(`mi_heap_visit_blocks` collects every page's span, `mach_vm_page_range_query` flags each 16 KiB page of each arena, and
a page counts toward the footprint when present, dirty, and not reusable, or paged out); and the park-collect runtime
(`CMDR_EXP_PARKCOLLECT=<ms>`). `mi_debug_show_arenas()` is no substitute for the page walk: it stops printing at a
heuristic last-used slice and hid ~360 MiB of resident pages in round 1.
