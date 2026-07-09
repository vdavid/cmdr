# M-append spike — findings & recommendation

Research spike for Cmdr `docs/specs/archive-browsing-polish.md` §3 / `archive-browsing-plan.md` §M-append.
All work in this scratch crate; nothing written into the repo. Verified on macOS 15 (Darwin 25.5.0),
Apple Silicon, APFS local disk, 2026-07-09. Tools: Info-ZIP `unzip` 6.00, `ditto`, Archive Utility.app,
7-Zip `7z` 24.x (Homebrew), `jar` (JDK present), `qlmanage`, plus the Rust `zip` 8.6 and `rc-zip` 5.4.1
(Cmdr's own readers).

## TL;DR / recommendation

**No-go on append-past-EOF as specced. Replace it with a "clone + tail-rewrite" fast-add path that is the
same speed, keeps 100% reader compatibility, leaves zero dead bytes, and needs no compaction machinery at
all.** Two hard problems sink the append-past-EOF layout, and both are intrinsic to it:

1. **`ditto -x -k` silently drops appended entries** (and silently resurrects deleted ones). `ditto` — the
   programmatic Archive Utility path — does a *forward local-file-header scan* and stops at the first
   non-LFH signature, which in the append-past-EOF layout is the OLD central directory sitting between the
   old data and the new entries. It never reaches anything appended past it. This is unfixable within the
   layout: crash-safety *requires* the old CD+EOCD to survive between old data and new entries, which is
   exactly the byte `ditto` can't cross.
2. **CD-rewrite delete leaves the deleted file physically recoverable.** "Delete = rewrite the CD omitting
   the entry, leave the data as dead bytes" means the deleted entry's bytes stay in the file. CD-honoring
   readers hide it, but `ditto` (forward scan) still extracts it, and anyone with a hex editor recovers it.
   That's a data-remanence / privacy regression versus today's temp+rename, which physically removes it.

The motivating win — "add a small file to a 2 GB zip without rewriting 2 GB" — is fully achievable **without**
either downside:

- **Local:** `clonefile` (APFS copy-on-write, measured **0.00 s** for 2 GB) the archive to a sibling temp,
  truncate at the old CD offset, write the new entries + a fresh contiguous CD + EOCD, then atomic-rename.
  Result is a **normal-structure zip** (entries contiguous, single CD, no dead bytes) that every reader
  including `ditto` accepts. Measured **0.149 s** to add 1 MB to a 2 GB zip — same ballpark as the raw
  append (0.159 s) and **~34x faster** than the 5.05 s full temp+rename rewrite.
- **Remote (SMB/NAS, the real motivation):** the server-side-copy analog. Create the temp on the share,
  `FSCTL_SRV_COPYCHUNK` the retained bytes server-side (no wire transfer), upload only the new entry, write
  the CD, rename. Wire cost drops from ~4 GB (download 2 GB + upload 2 GB) to ~1 MB + CD. smb2 has the
  copychunk message layer already; it needs a client-level API (see Risks).

Keep DELETE / rename / mid-archive edits on the existing O(archive) temp+rename mutator (physical removal,
no remanence). Only tail-adds get the fast path. David's stated case is adding, so this covers it.

## Reader-compatibility matrix

Scenarios (all from a 4-entry base zip): S2 = append 1 file; S3 = append then delete a different file;
S4 = two successive appends (two dead CDs stacked); plus a system-`zip`-made archive with directory
entries. "append-past-EOF" = the specced hand-rolled layout. "clone+tail" = the recommended design.

Reader × operation → result:

- `unzip -t` (integrity): PASS on every append-past-EOF and clone+tail archive.
- `unzip -o` (extract + byte-exact probe): PASS on every archive, both layouts.
- **`ditto -x -k`**: **FAIL on every append-past-EOF archive** — extracts only the pre-existing entries,
  exit code 0 (silent). **PASS on every clone+tail archive** (added file present, byte-exact).
- **Archive Utility.app** (real GUI, `open -a`): **PASS on append-past-EOF too** — S2, S3 (delete
  reflected: `dir/file1.txt` gone), and S4 all extracted correctly with the right entries and byte sizes.
  This is the path Finder double-click uses, so end-user double-click is fine; `ditto` is the CLI/scripting
  path that breaks.
- `7z t` / `7z x` (test + extract): PASS on every archive, both layouts.
- `jar tf` (list): PASS on every archive.
- Rust `zip` 8.6 (open + read every entry): PASS on every archive, both layouts — **Cmdr's own writer-side
  reader accepts it.**
- `rc-zip` 5.4.1 (sans-IO FSM, Cmdr's browse/extract reader): PASS on every archive, both layouts —
  **Cmdr's own read path accepts it.**
- `qlmanage -t` (Quick Look thumbnail): **INCONCLUSIVE in this environment** — the QL zip thumbnail
  generator hangs (>30 s timeout) on *every* zip here, including an untouched normal one, while it
  thumbnails a plain `.txt` in <1 s. So it's a headless-session QL-daemon issue, not layout-specific.
  Quick Look reads the central directory (same family as Archive Utility.app, which passed), so risk is low,
  but this needs a manual check on a real desktop before shipping.

Bottom line: the append-past-EOF layout is rejected by exactly one common macOS tool (`ditto`) and it's a
silent, data-losing failure. The clone+tail layout passes everything tested.

## Performance

Local, adding 1 MB to a 2 GB stored-content zip (Apple Silicon, APFS SSD):

- append-past-EOF: **0.159 s**, wrote ~7 KB (payload deflated) + CD + EOCD.
- clone+tail-rewrite: **0.149 s** total (clonefile 0.004 s + truncate/write/fsync 0.145 s), 0 dead bytes.
- baseline full temp+rename rewrite (`zip` `raw_copy_file` all entries + add): **5.049 s**, wrote a fresh
  2 GB.
- `clonefile` (`cp -c`) of the 2 GB archive alone: **0.00 s** (COW; `du` confirms shared blocks).

So the fast add is ~**34x** faster locally than the full rewrite, and the clone approach costs nothing over
the raw append while being fully compatible. On slower media (spinning disk, older SSD) the gap widens
because the baseline is bytes-copied while the fast path is O(new file).

SMB / NAS (analytical — deterministic, not benchmarked; standing up the Docker SMB fixture + writing an
offset-write/copychunk client was out of scope for the spike, and the byte counts are exact by construction):

- Today's remote flow (M5: pull whole archive local → temp+rename → upload whole): **download 2 GB +
  upload ~2 GB ≈ 4 GB over the wire.** At ~113 MB/s (gigabit) that's ~72 s of transfer; on Wi-Fi far more.
- copychunk + tail-rewrite (recommended remote design): **~1 MB (the new entry) + ~2× the CD size** over
  the wire (CD ≈ 60 bytes/entry: ~6 KB for 100 entries, ~600 KB for 10 000). The 2 GB retained bytes are
  copied *server-side on the NAS*, never traversing the network. **~4 GB → ~1 MB, ~3600x less data,
  sub-second vs ~72 s.**
- append-past-EOF over SMB offset writes would move the same ~1 MB + CD, but produces the ditto-incompatible
  layout on the NAS file. copychunk gets the identical wire win with a clean, compatible result.

## Edge questions

- **zip64:** handled. The parser resolves a zip64 EOCD + locator when the classic EOCD fields are saturated,
  reads per-record local-header offsets from the zip64 extra (0x0001, honoring which classic fields are
  saturated), and the writer emits a zip64 EOCD + locator and zip64 extras for new entries when any offset
  or size exceeds 4 GB or the record count exceeds 65 535. Old records are copied verbatim, so their
  existing zip64 extras are preserved untouched. Not exhaustively fuzzed, but the >4 GB append path is
  wired, not punted.
- **Data-descriptor entries (GP flag bit 3):** handled for retained entries — the CD record is copied
  verbatim and the local data (including any trailing descriptor) stays in place, so retention is
  byte-identical regardless of the flag. New entries are written with real sizes in the LFH (no descriptor).
  Verified end-to-end on a system-`zip`-made archive (directory entries + `proj/` tree): both layouts
  round-trip through `unzip -t`, `zip`, `rc-zip`, and (clone+tail) `ditto`.
- **Dead-space growth over N edits (append-past-EOF only):** each append orphans the previous CD + EOCD.
  Because each new CD restates *all* prior entries, the dead space grows roughly O(N²·k·~60 bytes) for N
  successive adds of k entries — the whole prior CD dies each time. Deletes additionally orphan the deleted
  entries' local data (potentially large). This is the entire reason append-past-EOF needs a compaction
  threshold. **The clone+tail design sidesteps this completely: zero dead bytes on every edit, so there is
  no dead-space accounting and no compaction feature to build, ship, or explain to the user.** If
  append-past-EOF were pursued anyway, David's 20% (dead/total) default threshold is reasonable — but note
  20% of a 2 GB archive tolerates ~400 MB of waste before a repack, so consider an absolute ceiling too.
- **Spotlight / `mdworker`:** not conclusively testable here (same headless-QL limitation). The zip
  metadata importer reads the central directory, and every CD-honoring reader accepted both layouts, so the
  risk is low — but verify on a real desktop, especially for the append-past-EOF dead-CD layout if it's
  ever revived.
- **CRC / offset gotchas hit:** (1) the EOCD scan must find the *last* EOCD signature, not the first, or an
  appended archive resolves to the stale CD. (2) Zeroing the old EOCD signature does NOT make `ditto` read
  appended entries — proof that `ditto` ignores the EOCD/CD entirely and scans LFHs forward; the old CD
  itself (not the EOCD) is the wall it stops at. (3) New-entry CD records must carry the real local-header
  offset (past 4 GB → zip64 extra), and retained records must keep their original offsets unchanged since
  their data hasn't moved — mixing these up silently misdirects extraction.

## Risks that remain

- **`clonefile` is filesystem-scoped.** It works only when the temp sibling is on the same APFS volume
  (always true for Cmdr's local macOS case). On non-APFS targets (exFAT/FAT USB sticks, some network
  mounts) `clonefile` fails; the code must detect that and fall back to the existing byte-copy temp+rename
  (correct, just not accelerated). Cheap to handle; don't assume clonefile always succeeds.
- **SMB copychunk needs smb2 client work.** smb2 has `FSCTL_SRV_COPYCHUNK` constants and ioctl
  pack/unpack, but no client-level server-side-copy API, no `FSCTL_SRV_REQUEST_RESUME_KEY` request, and the
  write path currently hardcodes `offset: 0` (no random-access writes). Adding a copychunk client method
  (request resume key from the source handle, issue copychunk to the dest) is the compat-preserving remote
  path; not present today. Also: not every SMB server supports copychunk (very old Samba, some NAS
  firmware) — need a capability probe with graceful fallback to the current pull-round-trip.
- **Quick Look and Spotlight are unverified here** (headless QL daemon). Low risk for the clone+tail
  (normal-structure) layout; must be confirmed on a real desktop before ship.
- **The spike's zip64 and data-descriptor handling is wired but lightly tested.** Before productionizing,
  add fuzz/property tests for >4 GB offsets, saturated counts, and mixed data-descriptor sources.
- **This crate is not production code.** No streaming (the appended entry is whole-buffered), no pause/cancel
  hooks, no MutationHooks seam, no metadata preservation, minimal error handling. It exists to answer the
  compat and perf questions, which it does.

## Reference implementation (local, not in git)

The spike crate is preserved at `_ignored/spikes/m-append/` (gitignored, this machine only): `src/lib.rs` holds the
hand-rolled EOCD/CD parse (incl. zip64), `append_files` (append-past-EOF), `delete_files` (CD-rewrite), and
`rewrite_tail_append` (the recommended clone+tail design); `src/bin/spike.rs` and `src/bin/cappend.rs` are the
drivers; `compat.sh` runs the reader matrix for one zip. Scratch quality — answers the compat/perf questions, not
production code.
