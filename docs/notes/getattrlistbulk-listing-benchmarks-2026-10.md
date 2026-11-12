# `getattrlistbulk` for the local listing: benchmarks and parity (2026-10)

The spike behind "Date added" and "Last opened" sorts (GitHub [#691](https://github.com/vdavid/cmdr/issues/691)). A sort
needs the date for EVERY entry, so the question is what each date costs across a whole listing, and whether moving
`list_directory_core` to `getattrlistbulk` pays for itself. It builds on `reanchor-cost-spike.md` (the index walker's
`getattrlistbulk` numbers); this note measures the LISTING, with the full `FileEntry` built.

Measured 2026-10-22 with `crates/cmdr-local/examples/listing_bench/`, first against a prototype lister, then against the
shipped one (§ "Production lister"). The harness now runs production's listers (`reading::list_directory_with`, bulk or
`read_dir` forced), so every number here is re-runnable against shipped code. The production lister's design:
`crates/cmdr-local/DETAILS.md` § "Two listers, one entry".

## Verdicts

- **Is the bulk listing surely faster at 100k?** Yes, on APFS: 1.4–1.6× warm (241 → 156 ms on the fixture, 252 → 185 ms
  on `_ignored/test-data/folder with 100000 files`) and 2.5–2.7× cold-ish (750–893 → 274–362 ms), and it brings
  `added_at` for free. ❌ Not on smbfs: 2.2× SLOWER warm (55 → 119 ms for 155 entries), so network mounts keep the
  current path. Doable with four guards (§ "What a production lister needs").
- **Is Finder's Last opened cheap enough to fetch for every entry?** No. One `getxattr(com.apple.lastuseddate#PS)` per
  file costs ~17 µs warm and 26–58 µs cold-ish, so +1.7 s on a 100k listing that otherwise takes 0.16 s (12×), and a
  network round trip per file on SMB (815 ms for 155 entries). And the xattr isn't even Finder's whole answer (§ "Last
  opened is not one xattr"). Fetch it on demand (visible rows, or once when the user picks that sort, with progress and
  cancel), never inline.
- **What does per-entry `getattrlist(ATTR_CMN_ADDEDTIME)` cost on top of the current path?** +3.5–3.9 µs per entry warm
  (100k: 241 → 595 ms, 2.5× the listing) and +5 µs cold-ish (750 → 1,248 ms). The bulk lister gets the same value at
  zero marginal cost, so this add-on is the worse option on every axis except code churn.

## Numbers

Median of interleaved rounds (one warm-up round, then 7–11; methods run round-robin so none gets a systematically warmer
cache). CPU is user + sys from `getrusage`. On APFS every lister is CPU-bound: CPU equals wall within 1% in every run,
so only wall is listed. Everything includes building the `FileEntry` (name, path, icon id, owner and group names) and
the `sort_listing` call.

- **1k fixture** (1,086 entries): current 2.13 ms · bulk 1.17 ms · bulk + xattr 11.52 ms · current + getattrlist 4.66 ms
  · the NSURL enrichment Cmdr had then (removed since) 8.27 ms.
- **10k fixture** (10,620): current 24.2 ms · bulk 15.4 ms · bulk + xattr 133.3 ms · current + getattrlist 55.5 ms ·
  NSURL 92.5 ms.
- **100k fixture** (100,620): current 241.4 ms (2.40 µs/entry) · bulk 156.1 ms (1.55) · bulk + xattr 1,881 ms (18.7) ·
  current + getattrlist 595.3 ms (5.92). NSURL in a separate run: 919.8 ms against current's 231.3 ms.
- **100k real test data** (100,000 `.md` files, no xattrs): current 252.5 ms · bulk 184.9 ms · bulk + xattr 1,942 ms ·
  current + getattrlist 635.1 ms.
- **100k cold-ish**, two rounds (fresh fixture and fresh process per method, one pass): current 750 / 893 ms · bulk 274
  / 362 ms · bulk + xattr 2,671 / 5,818 ms · current + getattrlist 1,248 / 1,318 ms.
- **Bulk buffer size** at 100k: 16 KiB 177 ms, 64 KiB 169 ms, 128 KiB 165 ms, 512 KiB 171 ms. Flat; 64–128 KiB is fine.
- **smbfs** (`/Volumes/testshare/test_photos`, 155 entries, QNAP over Tailscale, 3 rounds): current 54.8 ms wall / 1.3
  ms CPU · bulk 118.6 / 1.4 · bulk + xattr 814.7 / 13.1 · current + getattrlist 63.1 / 1.9. First pass: current 1,056
  ms, bulk 848 ms. Warm `lstat` is answered from smbfs's attribute cache; `getattrlistbulk` re-enumerates over the wire.

### Method and environment

- Apple M3 Max, 64 GiB, macOS 27.0.1 (26A434), release build. Fixtures on the internal APFS Data volume (the session
  scratchpad under `/private/tmp`, which Spotlight doesn't index and a dev Cmdr doesn't watch).
- The machine was shared: load average 7–9 from other sessions throughout. Ratios held steady across reruns; absolute
  numbers on an idle machine will be somewhat lower.
- Fixture (`listing_bench gen`): N files (90% empty, 10% 1–8 KiB, 1% dot files, 1% `UF_HIDDEN`, 30% carrying the
  last-used xattr, which is the 39-of-99 ratio a real Downloads folder showed), plus `clamp(N/30, 30, 300)` dirs, as
  many symlinks (a third each to a file, to a dir, and dangling), and 20 hard links.
- **Cold-ish only.** `purge` needs `sudo`, which an agent session doesn't have. "Cold-ish" is a freshly generated
  fixture listed once by a fresh process; the generation itself leaves metadata in memory, so this is "first listing
  after the files appeared", not "first listing after boot". Every cold-ish run was still CPU-bound (CPU ≈ wall), so the
  first-pass penalty is in-kernel object setup, not disk IO. For a true cold number, run `sudo purge` before each
  `bench <dir> 0 <method>`.

### Reproducing

```sh
cargo build --release -p cmdr-local --example listing_bench
B=target/release/examples/listing_bench
$B gen /private/tmp/lb/f100000 100000
$B parity /private/tmp/lb/f100000 / /Volumes /usr/bin /private/var/db
$B bench /private/tmp/lb/f100000 7 current,bulk,readdir,bulk_xattr
$B volcaps / /Volumes/<some mount>
```

Methods now: `current` is production (bulk on APFS and HFS+), `bulk` and `readdir` force a lister, and `readdir` on APFS
includes one `getattrlist(ATTR_CMN_ADDEDTIME)` per entry (what the old `current_added` measured). The numbers above the
"Production lister" section came from the prototype and the pre-bulk `current`; to compare against the pre-bulk lister,
build the example at a commit before the bulk lister landed.

## Parity

`listing_bench parity` diffs every `FileEntry` field (with `added_at`/`opened_at` set aside) between the current lister
and the prototype, and checks the bulk `added_at` against an independent per-path `getattrlist`.

**Zero differences** on the 100k fixture, the 100k test data, `/usr/bin`, `/Applications`, `/System/Library/Frameworks`,
`~/Downloads`, `~/Library`, `/System/Volumes/Data`, an HFS+ image, an exFAT image, and two smbfs directories (verified
on macOS 27.0.1, `listing_bench parity`, 2026-10-22). `added_at` agreed with per-path `getattrlist` on every entry
except mount points and firmlinks (below).

Each of these bit the prototype and each is a guard a production lister must carry:

- **`FSOPT_PACK_INVAL_ATTRS` packs every REQUESTED attribute**, an unsupported one as zeros with its bit cleared in
  `ATTR_CMN_RETURNED_ATTRS`. Walk the requested set to find offsets; read the returned set only for validity. Walking
  the returned set shifted every later field by 16 bytes wherever `ATTR_CMN_ADDEDTIME` is unsupported (smbfs, exFAT,
  FAT), turning sizes into `4294967296`.
- **The dir group packs for directories only, the file group for everything else** (a symlink gets file attributes).
  This is the same lesson `reanchor-cost-spike.md`'s Go parser records.
- **Order is bit order, with one exception:** `ATTR_CMN_ERROR` packs right after the returned set, before the name.
  `ATTR_FILE_ALLOCSIZE` (0x4) precedes `ATTR_FILE_DATALENGTH` (0x200); getting it backwards swaps size and physical
  size.
- **`ATTR_CMN_ACCESSMASK` carries the `S_IFMT` bits on APFS** (`0o100555`), matching `st_mode`. The prototype masks to
  `0o7777` and ORs in bits from `ATTR_CMN_OBJTYPE`, which is right whichever way a filesystem answers.
- **Mount points and firmlinks report the directory they cover.** At `/`, 9 of 22 entries (`Users`, `Applications`,
  `Library`, `Volumes`, …) came back with the system volume's stub times, owner, and mode; at `/Volumes`, both mounts.
  Detect with `ATTR_DIR_MOUNTSTATUS & DIR_MNTSTATUS_MNTPOINT` or `ATTR_CMN_FLAGS & SF_FIRMLINK` (0x00800000) and re-stat
  just those; parity was zero after that. Their `added_at` then still comes from the stub.

Edge cases, with what each lister does:

- **Read but no search permission** (a `0444` dir): `read_dir` lists the names and every `lstat` fails, so the current
  path shows five minimal rows. `getattrlistbulk` fails the WHOLE call with `EACCES`. A production lister must fall back
  to the current path on that error.
- **No permission at all**: both fail at `open`, same error.
- **`EPERM` entries** (`/private/var/db/DifferentialPrivacy`, sandbox-protected): `lstat` gets `EPERM`, so the current
  path shows a minimal "file" row; `getattrlistbulk` returns its real attributes, so it lists as a folder. Bulk is the
  more honest answer, but it is a visible change.
- **Entries vanishing mid-listing**: deleting all 2,000 files after the first 4 KiB batch made the next call return 0
  with no error (73 listed). The current path has a wider window (between `readdir` and each `lstat`) where a vanished
  entry becomes a minimal row.
- **`ATTR_CMN_ERROR` entries**: never seen in any listing here; the prototype maps them to the current minimal row.
  Untested.
- **`EINTR`**: the prototype retries; never observed, and the man page doesn't say whether a retry can skip a batch.
  Untested.
- **Non-UTF-8 names**: APFS refuses them (`EILSEQ`), and both listers decode through `to_string_lossy`. No difference.
- **`/.hidden`**: read once per volume-root listing, independent of the lister. No difference.
- **FAT32 (`msdos`)**: `ATTR_FILE_ALLOCSIZE` reads 0 where `st_blocks * 512` is 4,096, so `physical_size` differs on
  every file. exFAT matched. Fall back for that field (or the whole volume) when its returned bit is clear.
- **TCC-protected folders** (Desktop, Documents without a grant): not tested; this process inherits the terminal's
  grants.

### Volume support

`listing_bench volcaps` reads `ATTR_VOL_CAPABILITIES` and `ATTR_VOL_ATTRIBUTES` (verified on macOS 27.0.1, 2026-10-22):

- **APFS, HFS+**: `VOL_CAP_INT_ATTRLIST` native, `ATTR_CMN_ADDEDTIME` valid and native.
- **smbfs**: attrlist native, `ADDEDTIME` NOT valid, so `added_at` is `None`. Bulk works but is slower.
- **exFAT, FAT32**: attrlist not native (the VFS emulates `getattrlistbulk`, and it works), `ADDEDTIME` not valid.
- Linux has no `getattrlistbulk`, so the `std::fs` path stays in the crate regardless.

### What a production lister needs

1. **Choose per volume, once.** Bulk for APFS and HFS+ (the `statfs` `f_fstypename` the crate already reads, or
   `ATTR_VOL_ATTRIBUTES`); the current path for smbfs, NFS, and other network filesystems. exFAT and FAT work through
   VFS emulation but gain nothing measured here and carry the FAT physical-size gap, so keep them on the current path.
2. **Fall back at runtime** to the current path when `getattrlistbulk` itself errors (`EACCES` above; also `ENOTSUP` /
   `EINVAL` as a guard).
3. **Re-stat mount points and firmlinks** (rule above).
4. **Validate by returned bit**, never by offset (rule above), and keep a parser test per filesystem shape: an
   unsupported attribute in the middle of the requested set is the case that broke.

## Production lister

Re-run against the shipped lister (commit `fc1f736ceb`), same machine and method as above, 2026-10-22, load average 2–3.
The "before" column is the pre-bulk production lister, from a release build of this example taken just before that
commit, run back to back on the same fixture.

- **100k fixture, warm** (100,620 entries, 7 rounds): production 160.4 ms (1.59 µs/entry) · bulk forced 158.8 ms ·
  `read_dir` forced 477.8 ms (4.75 µs/entry: `lstat` plus one `getattrlist` for the added date, the fallback's cost) ·
  before 228.1 ms (2.27 µs/entry). So 1.42× faster than before, with `added_at` on every entry. The prototype measured
  149.1 ms in the same run; the shipped lister's ~7% extra is the progress tally and the trust checks per record.
- **100k cold-ish** (fresh fixture, fresh process, one pass, one round each): production 167.6 ms · before 252.4 ms
  (1.5×). The lighter load here makes both far quicker than the cold-ish rounds above, so read the ratio, not the
  absolutes.
- **Parity** (`listing_bench parity`, production bulk vs production `read_dir`, every field, `added_at` included): zero
  differences on the 100k fixture, `/`, `/Volumes`, `/usr/bin`, `/Applications`, `/System/Library/Frameworks`,
  `~/Library`, `~/Downloads`, and `/System/Volumes/Data`. `/private/var/db`: zero differences plus one sandbox `EPERM`
  entry, the intended difference (bulk lists it as the folder it is, `read_dir` as a minimal row). `added_at` agreed
  with a per-path `getattrlist` on every entry except that one (whose `getattrlist` gets `EPERM` too). At `/` and
  `/System/Volumes/Data`, one entry has no added date on either lister.

## What a re-stat costs, and why it keeps its `statfs`

`get_single_entry` (the watcher's re-stat, and every write path's preflight, cleanup, and conflict check) asks "is my
folder bulk-eligible?" with a `statfs` of the parent on every call, then one `getattrlist` for the added date where it
is. A per-device cache keyed on the call's own `lstat` `st_dev`, cleared by a thread blocked on a kqueue `EVFILT_FS`
filter, was built and measured, then reverted. Median of 5–7 warm passes over every entry, release build, macOS 27.0,
2026-10-23, load average 3–5:

- **APFS, 10,620 entries**: 4.47–4.50 µs per call with the `statfs`, 3.30–3.36 µs with the cache. The `statfs` is ~1.2
  µs of each call.
- **smbfs, Docker Samba on localhost** (3 entries): 2.56 µs vs 1.47 µs.
- **smbfs, a NAS over Tailscale** (18 entries): 3.44 µs vs 1.62 µs.

On both smbfs mounts the warm `statfs` answered from the client's cache, so there's no network round trip per call in
steady state: the cache saves one syscall, not a round trip. **Decision: keep the `statfs`.** ~1 µs per call doesn't pay
for ~230 lines, a resident thread, and device-number-reuse and mid-read-mount races to reason about. Revisit only if a
profile shows re-stats hot, or a cold `statfs` on a slow mount turns up in one. (An inline kqueue drain before each
lookup was worse than the `statfs`: 15.9–16.9 µs wall per call, because the call yields.)

## Last opened is not one xattr

Finder's "Date Last Opened" is Spotlight's `kMDItemLastUsedDate`. The `com.apple.lastuseddate#PS` xattr is one of its
inputs: 16 bytes, a little-endian `struct timespec` (`i64` seconds, `i64` nanoseconds), decoded and compared against
`mdls` on eight files in `~/Downloads` (verified on macOS 27.0.1, `xattr -px` + `mdls`, 2026-10-22). What else showed:

- Most matched to the second. One file's xattr was 6.8 hours OLDER than Spotlight's value.
- Two files had a Spotlight last-used date and no xattr at all (an `.xlsx` and a `.pdf`).
- No `.app` carries the xattr, yet Spotlight holds a last-used date for most apps (LaunchServices feeds it).
- `atime` (what Cmdr's since-removed NSURL enrichment read through `NSURLContentAccessDateKey`) is a third, unrelated
  value: one file's atime was 2026-10-16 against a last use of 2026-07-27.

So the xattr alone would show a blank or stale "Last opened" for a real share of entries. The faithful source is a
scoped Spotlight query: `mdfind -onlyin ~/Downloads 'kMDItemLastUsedDate == *'` took 40–80 ms including process launch,
recursive. An in-process `MDQuery` scoped to one directory wasn't measured; it's the candidate worth pricing next. It
returns nothing for unindexed locations (this scratchpad, external drives with indexing off, network shares).
