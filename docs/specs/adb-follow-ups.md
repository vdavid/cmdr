All confirmed in the code on 2026-09-11

## 1. Rescanning a phone

I meant the index; nothing else is affected. A phone's folder listing in a pane never updated live anyway, because ADB
doesn't report changes, so you see them after a refresh.

I'm withdrawing the recommendation: you already decided this on 2026-08-16. It's recorded in
`crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` ("refreshing the index for a non-boot drive stays the user's to
trigger"). So today's behavior is on purpose.

## 2. Confirm under the red notice

It's the copy/move dialog (F5/F6). When the destination folder takes no writes, a red "This folder doesn't accept files"
shows under the path box. Confirm stays pressable, and pressing it starts the copy, which refuses right away with the
same reason in an error dialog. Confirm is disabled today only for path errors.

- **Keep it enabled:**
  - **Pro:** a stale answer still lets you try (for example, you fixed permissions after the check ran).
  - **Con:** a button that always leads to a refusal is a dead end with an extra dialog.
- **Disable it:**
  - **Pro:** red already means blocked in this dialog, since path errors disable Confirm.
  - **Con:** a wrong answer leaves no way to try. But keeping it enabled doesn't help there either: the backend asks the
    same question before writing (`copy.rs:421`) and refuses anyway.
- **My take:** disable it, with the notice as the reason. A clear win, about 10 lines.

## 3. The index registry lock

The bug is real, but it's milder than I made it sound.

- **What happens:**
  - The first time a drive's index walk starts (or a search walks unindexed folders, or you stop a scan), Cmdr starts a
    macOS file watcher while holding the index's global lock.
  - Starting the watcher waits on `fseventsd`.
  - It also reads the database twice under the lock, which the module's own docs forbid.
- **Who waits:**
  - Everything that asks about any index: status, badges, MCP, and the check run after each navigation.
  - They wait on background threads, never the UI thread. In theory, enough waiters would stall every backend call,
    which would look like a frozen app.
- **How bad in practice:**
  - It happens about once per drive per session.
  - Your last week of logs shows it took 24–63 ms.
  - It only reaches seconds on a machine busy with builds and test runs, which is how the tests exposed it.
- **Fix:**
  - Decide under the lock, start the watcher outside it, then re-lock and install it only if nothing changed meanwhile.
  - The hard part is races: two walks both starting a watcher, or a teardown landing in between.
  - About 150–250 lines, medium risk, and it needs a fake watcher to test, which is also what #4.1 needs.
- **My take:** not urgent. Do it together with the fake watcher.

## 4. Flaky tests: impact

Since 2026-09-01, the Rust tests lane had 178 clean runs (68 s on average) and 157 warning runs, where a test failed in
the suite, passed on retry, and made the run take 164 s. None of the 99 red runs was caused by these tests. So the cost
is noise plus about 1.5 minutes of retries on roughly half the runs, never a false red.

1. **Index phase tests (39 tests):**
   - **Impact:** the biggest by far. Named in 109 runs, up to 25 a day, 12 even today.
   - **Cause:** they start real macOS watchers and wait up to 30 s, under an 8 s test limit.
   - **Fix:** (a) a 20 s limit for them, five lines, a band-aid; (b) a fake drive watcher, 150–300 lines, shared with
     #3.
2. **Git watcher test:**
   - **Impact:** named in 78 runs since 2026-09-06, 14 today.
   - **Product impact:** each of the four paths watched per repo restarts the macOS event stream. So the git chip
     appears 0.1–0.5 s late the first time a pane enters a repo, and under heavy load it can miss its 2 s budget and not
     show at all.
   - **Fix:** one stream for all four paths through our vendored `fsevent-stream`, 150–250 lines.
3. **Four trash tests:** failed twice ever in real runs. Negligible; a five-line limit bump covers them.
4. **`target/debug/deps` size:**
   - **Now:** 63,658 files, down from 117,078.
   - **Why it dropped:** your `cargo-sweep` job already runs daily at 04:51, keeps four days, and freed 26 GiB this
     morning. It skips any night a `cargo` process is running.
   - **Options:** (a) keep one or two days; (b) sweep even while agents build.
5. **Two WebDAV tests:** nothing slow in the code (no retries, no waits). The Docker fixture stalled for 18 tests at
   once under load. I'd time them once on an idle machine before touching anything.

## 5. File splits: genuine win or not

1. **`volume_copy.rs`:** genuine win.
   - **Why:** 318 lines of conflict pre-check logic plus 375 lines of its tests sit in the IPC commands layer, which its
     own `CLAUDE.md` says holds no business logic.
   - **Move:** into `write_operations/conflict_precheck.rs` plus a tests file, leaving about 290 lines.
   - **One detail:** it needs the deadline helpers from `commands/util.rs`, which I'd move down a layer with it.
2. **`ids.rs`:** a genuine win for one block.
   - **Why:** about 125 lines build and parse app path schemes (`adb://`, `sftp://`, `webdav://`), a different concept
     from volume IDs.
   - **Move:** into `remote_paths.rs`, which already owns "the two spellings of a remote path".
3. **`cover/network_tests.rs`:** not a win. It's one subject. Trim about 45 lines of repeated setup instead.
4. **`volume-capabilities.test.ts`:** a genuine win, but on the source file.
   - **Why:** `volume-capabilities.ts` holds a separate archive-path module of about 160 lines, and eight files import
     only that part.
   - **Move:** into `archive-paths.ts` with its own tests, which then need no store or settings mocks.
5. **`clipboard-operations.test.ts`:** not a win; trim about 50 lines of repeated mocks. The agent also found a test
   that no longer tests what it claims:
   - **What it pins:** the "MTP refusal" test compares against an old `mtp-` prefix check.
   - **The real rule:** the gate now refuses MTP, ADB, SFTP, and WebDAV, so the test passes without covering three of
     the four.
   - **Fix:** a small table test.
6. **e2e `CLAUDE.md`:** a doc condense. Moving the incident stories into its `DETAILS.md` gets it to about 400–450
   words.

## 6. Leftovers, PISS

**1. Copy preview from a phone that's gone**

- **Problem:** the preview treats a volume it doesn't know as a local disk, walks the literal `adb://…` path, and fails.
  The dialog says "We couldn't finish measuring the source" with a Retry that never works.
- **Impact:** low.
  - **Where:** the only way there is a search results pane over a phone's index after unplugging the phone.
  - **Confirm afterwards:** it says "Not connected yet" for a phone that was never dialed, but a generic "Source volume
    not found" for an unplugged one.
  - **F8:** starts the same preview.
- **Solution:** the preview refuses unknown volumes with a typed reason, shows the "Not connected yet" copy, and hides
  Retry.
- **Size:** about 70 lines plus two tests, low risk.
- **Clear win.**

**2a. Bulk rename's plain-text errors**

- **Problem:** plain English strings. Only agent suggestions reach it, and the dialog only logs the result.
- **Impact:**
  - **The strings themselves:** nobody ever sees them.
  - **The bigger find:** when an approved suggestion's operation refuses to start, the suggestion is already marked
    approved. It disappears from the list with nothing running and no message. That's true for every operation type, not
    only renames.
- **Solution:** (a) type the errors, about 30 lines; (b) give the approval back, or show why it didn't start.
- **Size:** (a) is small and low risk. (b) needs a small design and copy decision from you.
- **Clear win for (a), but it's tiny; (b) is the valuable part, and a tradeoff.**

**2b. `SourceVolumeGone` when approving a suggestion**

- **Problem:** it calls a phone or server that isn't connected "gone", and the frontend handles neither case.
- **Impact:** real but uncommon. You approve a suggestion about a NAS share or phone after it went away, the click does
  nothing, and nothing says why.
- **Solution:** classify with the existing not-connected helper and show it in the suggestions dialog.
- **Size:** about 55 lines, and the dialog's first visible refusal, so it needs your review of the copy.
- **Mostly a clear win.** It pairs well with 2a (b).

**2c. "Volume not found" while listing**

- **Problem:** the error carries a sentence where it should carry the path, and a test string-matches it, which the repo
  forbids.
- **Impact:** almost none. It only happens in an unmount race, the pane already recovers sensibly, and the English shows
  only under technical details.
- **Solution:** (a) carry the path and fix the test; (b) also unify the three shapes "gone" has across the code.
- **Size:** (a) five lines; (b) about 15 match arms plus i18n.
- **(a) is a clear win, (b) a tradeoff.**

**3. Write access on direct SMB**

- **Problem:** direct SMB answers "don't know", so a read-only share shows no notice.
- **Impact:** low to moderate, since read-only media shares and guest logins are common.
  - **Direct shares:** the copy starts, then fails on the first write with "You don't have permission to copy files
    here". That's honest but late, with no data risk.
  - **Possible inconsistency:** the same share may show the notice while still on the macOS mount and lose it after the
    upgrade to direct. That's unverified; `test -w /Volumes/<share>` on a read-only share would settle it.
- **Solution:**
  - **The gap in `smb2`:** it decodes the share's access rights at connect and then discards them, and it doesn't
    support the per-folder access query at all.
  - **Short term:** (a) keep the share-level rights as a free early answer.
  - **Real fix:** (b) add the per-folder query to `smb2`, and have `cmdr-smb` ask it for the nearest existing folder.
- **Size:** (a) about 40 lines; (b) about 170–230 lines across both repos plus an `smb2` release.
- **Tradeoff, leaning win.**

## 7. The `ld: duplicate symbol` warnings

They predate the branch; they're from 2026-07-30. They only affect the test binary, and the shipped app is fine.

- **Cause:**
  - `apps/desktop/src-tauri/Cargo.toml:426` makes the app a dev-dependency of itself,
    `cmdr = { path = ".", features = ["testing"] }`. So the unit-test binary links two copies of the app: its own test
    build and the `testing` build.
  - Ordinary Rust symbols don't clash between the two, but objc2 gives each Objective-C class fixed, unmangled symbol
    names. It does that on purpose, so duplicate classes surface at link time.
  - So `CmdrPromiseDelegate`, `QuickLookDelegate`, `CmdrDragSource`, and Tauri's embedded `Info.plist` collide. The
    linker keeps one of each and warns.
- **Risk today:** none. Both copies have identical class layouts. A test-only field added to one of those classes would
  turn this into memory corruption in tests.
- **The interesting part:**
  - The self-dependency was added for `index_benchmarks.rs`, which moved to `crates/cmdr-index/benches/` the very next
    day.
  - Its reason has been gone since. Today it only switches on `testing` for nine workspace crates, and `cmdr-fs` already
    gets that the direct way (`Cargo.toml:429`).
- **Fix:**
  - Replace the self-dependency with direct dev-dependencies that turn on `testing` for those crates, and delete the
    outdated comment.
  - Resolver 2 keeps dev features out of the shipped build.
  - Size: about 25 lines in `Cargo.toml` plus one `cfg` line. Verify with clippy on all targets plus the Rust tests.
- **My take:** clear win, low risk. It also stops the 340 MB test binary carrying part of a second copy of the app.
