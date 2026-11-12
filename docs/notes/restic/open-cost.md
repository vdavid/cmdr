# What opening a real restic repo costs

Evidence for the restic volume's index lifecycle (#422). The decisions it fed live in `crates/cmdr-restic/DETAILS.md` §
"The index lifecycle". Measured with `crates/cmdr-restic/examples/open_cost.rs`, which drives `rustic_core` directly
through a counting copy of the crate's storage adapter (every write refused and counted: zero across every run).

## Setup

- **Where**: rustic_core 0.13.0, release build, macOS 27.0.1 on an M3 Max (16 cores), 2026-10-20.
- **Repo**: `/Volumes/testshare/backups/test-restic-repo` on the QNAP TS-464 over Tailscale on a direct LAN path (ping
  RTT ~3.6 ms). Nine snapshots, one key file, 45 index files totalling 112,303,608 bytes, 2,382,907 blobs, 131.9 GiB
  stored (261.8 GiB uncompressed). The latest snapshot is ~182 GiB.
- **Two parents**: `mount` is a `LocalPosixVolume` over the OS smbfs mount; `smb` is a direct `cmdr-smb` session (smb2
  0.32.1) to the same share. Connecting the direct session took 0.11–0.12 s.
- **Method**: one phase per process under `/usr/bin/time -l`, so peak RSS is that phase's alone. Every number ran at
  least twice; both are listed. `no_cache` unless the row says otherwise.
- ❗ **The mount's second runs are the macOS smbfs CLIENT cache**, not the NAS's: an index load at 350 MB/s beats the
  wire. Read the mount's first run as cold.

## Numbers

Unlock and snapshot list (no index):

- **Unlock** (`open`: scrypt + one key + config): 0.41–0.61 s across all 20+ runs, either parent. One `keys/` file.
- **`get_all_snapshots`**: nine snapshots, 4,551 bytes. Mount 0.088 s cold, 0.016–0.027 s warm; SMB 0.033–0.050 s.
- **Peak RSS, open + snapshots**: 44–46 MB for the whole process.
- **Listing `index/`**: mount 0.24 s, SMB 0.023–0.026 s.

Index load (both read all 45 index files, 112.3 MB):

- **`to_indexed_ids`**: mount 1.61 s cold, 0.32 s warm; SMB 2.94 s and 2.52 s. Peak RSS 555–620 MB.
- **`to_indexed`**: mount 2.07 s cold, 0.20–0.44 s warm; SMB 4.25 s and 3.97 s. Peak RSS 575–690 MB.
- **Peak concurrent parent reads during the load: 16**, every run, which is rayon's global pool on this 16-core Mac.
  Each one is a pool worker blocked in our adapter for a whole ~2.5 MB index file.
- Peak RSS is ~5x the index files' total size (560 MB / 112 MB), ~240 bytes per blob.

Browsing and reading (latest snapshot):

- **Top three levels** (160 trees, 672 nodes, 160 ranged pack reads, 741 KB): mount 0.61 s cold, 0.03–0.11 s warm; SMB
  2.16–2.30 s, the same on every round. That's ~14 ms per tree on SMB (about four round trips) against ~3.8 ms through
  the mount.
- **Under `to_indexed_ids` there's no blob cache**: walking the same 160 trees again re-reads all 160 ranges.
- **100 MiB of a 1.7 GB already-compressed file** (4 MiB `read_file_at` calls, one pack read per blob, ~1.4 MB each, 129
  MB of packs read for 105 MB of content): mount 34.5 MB/s cold, 143–146 MB/s warm; SMB 14.2, 19.1, 19.3, and 20.3 MB/s.
  Finding the file took a 4–16 s tree search, not part of the read.
- A first try picked a 470 MB sparse disk image: 100 MiB of it was 2.7 MB of packs. Throughput needs an incompressible
  file, which is why the probe looks for one by extension.

rustic's cache (`cache_dir` set, then `to_indexed_ids` + the three-level walk):

- **Cold**: the index load is the same read (mount 1.83 s; SMB 4.86 s and 5.51 s), but the walk fetches **whole tree
  packs** into the cache: 16 reads, 264.8 MB, for the 160 trees the uncached walk got from 741 KB. Walk time mount 4.88
  s, SMB 5.57–5.78 s. Peak RSS 794–825 MB.
- **Warm**: zero repo reads after unlock. Index load 0.25 s, walk 0.009 s, total process 0.72–0.87 s, RSS 539–554 MB.
- **Cache size after one walk**: 360 MB (index 107 MB, tree packs 253 MB, snapshots 36 KB).

Baseline, `restic` 0.19.1 CLI over the mount, `--no-lock --no-cache`:

- **`snapshots`**: 0.85 s and 0.47 s, 63 MB RSS.
- **`stats --mode raw-data`**: 194.8 s and 170.2 s, 2.34–2.36 GB RSS. It walks every tree of all nine snapshots, so it's
  a ceiling, not a peer of our index load.

## Recommendations

- **(a) Load with `to_indexed` up front, and skip `to_indexed_ids`.** Both read the same 112 MB. Full costs ~0.5–1.5 s
  more and ~40–90 MB more RSS here, while upgrading ids to full later is `drop_index` + a second full read (another ~3–4
  s on SMB) plus a fresh unlock. Copy-out and the viewer need full anyway, and full is the state with the blob cache.
- **(b) Hub cap: one indexed repo at a time, and up to eight unlocked-but-unindexed repos.** An index is ~0.5–0.65 GB
  for this 2.4M-blob repo; an unlocked repo with its snapshot list is a few MB. Opening a second index drops the least
  recently used one. Budget by size too: estimate the index's memory as ~5x the `index/` files' total (the listing gives
  it before any load) and show it in the progress, since a 10M-blob repo would want ~2.5 GB.
- **(c) `no_cache`.** The cache makes the first browse slower (the walk reads 265 MB of tree packs and takes 2.5–8x
  longer), raises peak RSS by ~240 MB, and writes 360 MB to disk per repo. Its only win is re-opening (index 0.25 s
  instead of 2–5 s on SMB), and within a session the in-memory index already covers that. If re-open cost bites later,
  cache the `index/` files ourselves in the adapter (112 MB here, no tree-pack download).
- **(d) Drop the index after five idle minutes** (no pane inside the view, no open reader, no running copy), keeping the
  snapshot list and the folder names. Getting it back costs an unlock (~0.5 s, password from Keychain, since rustic
  can't re-open from the master key) plus a load (~2–4 s on SMB, under a second on a warm mount), which the progress
  phase covers. Holding ~0.5 GB idle for longer isn't worth that.
- **(e) Stream the index files, at every size.** Holding them all past ~256 MB of `index/` costs too much, and letting
  rustic read them parks rayon's global pool; § "Streaming the index load" has what replaced both.

Two findings for the volume milestones:

- **`SmbVolume::read_range` is open + read + close per call** (`crates/cmdr-smb/src/volume/volume_impl.rs`), which is
  the ~14 ms per tree and the serial ~20 MB/s copy-out. The adapter now holds `open_random_reader` handles per pack (up
  to 8, least recently used let go first); § "Held pack readers" has the before and after. Copy-out should still read
  several blobs ahead in parallel.
- **The index load blocks all 16 rayon workers** for 2–5 s on SMB, as rustic's source predicted
  (`crates/cmdr-restic/DETAILS.md` § "rustic_core 0.13.0 facts"). Search and icons share that pool. The load now runs
  rustic on a pool of its own; § "Streaming the index load".

## Held pack readers

The same probe with `PROBE_HELD_READERS=1`, which reads packs the way the crate's adapter now does: one
`open_random_reader` per pack, held (up to 8, least recently used let go first), instead of one `read_range` per window.
Direct SMB session only, since a held reader is what SMB lacks; the mount's `read_range` is a `pread` on an open file
already. Same Mac, NAS, repo, and release build, 2026-10-20, held and not held interleaved, each phase run twice.

- **Top three levels** (160 trees, 160 pack reads, 741 KB): 2.18–2.29 s before, 0.88–0.97 s after: ~14 ms down to ~5.8
  ms per tree, 2.4x faster. Every read is still one call; only the open and close are gone.
- **Finding a 100 MiB file** (the BFS tree search): 16.4 s and 16.4 s before, 6.1 s and 6.1 s after.
- **100 MiB of the 1.7 GB file** (90–91 pack reads of ~1.4 MB): 15.9, 17.4, 19.5, and 21.1 MB/s before; 24.7, 25.0,
  26.3, and 26.4 MB/s after, about 1.3x. What's left is reading one blob at a time: each 1.4 MB read waits for the last.
  Reading several blobs ahead in parallel is the next lever for copy-out.
- Zero write attempts in every run.

## Reading blobs ahead

The same probe's `full` phase with `PROBE_CHUNK_KIB=1024` (the read stream's window) and `PROBE_READ_AHEAD=N` windows in
flight, handed back in order the way a stream hands them to a copy. Held pack readers on, direct SMB session, same Mac,
NAS, repo, and release build, 2026-10-20; each run reads the same 100 MiB twice (rounds 1 and 2), and the one-at-a-time
and four-ahead runs were each repeated.

- **One window at a time** (what the stream did): 22.9, 21.2, 22.1, and 19.7 MB/s. 105–110 pack reads, 149–163 MB of
  packs for 105 MB of content.
- **Four ahead**: 29.7, 39.3, 38.8, and 41.0 MB/s, about 1.8x. 84–88 pack reads, 111–115 MB of packs.
- **Eight ahead**: 35.8 and 39.2 MB/s, no better than four.
- The serial run reads ~40% more pack bytes for the same content: some blobs straddling two windows are fetched twice
  despite rustic's 32 MB blob cache (cause not traced). Concurrent windows asking for the same blob share one fetch
  (`quick_cache`'s `get_or_insert_with` holds a placeholder), which fits the read-ahead's lower byte count.

So the read stream keeps four 1 MiB windows in flight: at most 4 MiB held per stream, and a copy's Cancel detaches at
most four reads.

## Streaming the index load

The crate's own index load (#630), measured with the probe's `load` phase: a `ResticRepos` hub and a `ResticVolume` over
the parent, an unlock, then a listing of `<repo>/Browse snapshots/latest`, which waits on the load the way a pane does.
Same Mac, NAS, repo, and release build, 2026-10-21; the repo had grown to 49 index files, 119,443,774 bytes. Before is
`56dd11f457` (every index file read ahead and held, rustic decoding on rayon's global pool); after streams them through
the 16 MiB window to five `restic-index-*` threads. Before and after ran interleaved.

- **Steady RSS** (3 s after the load, the index still loaded; `phys_footprint` within ~20 MB of it): SMB before 518,
  593, 628, 630, 630, 653 MB; after 395, 595, 599 MB. Mount before 624, 625, 628, 630, 656, 667 MB; after 581, 585, 592,
  619, 620, 623 MB. About 5x the index files, either way.
- **Peak RSS** matched steady within a few MB in every run but two (SMB before: 625 peak against 518 steady, 633 against
  593). The peak is rustic's own, at the end of the build, after the old prefetch had already handed its bytes over. So
  on this repo streaming saves ~30–40 MB of steady RSS (fewer freed buffers left in the allocator) rather than lowering
  a peak; what it removes is holding the whole `index/` at once, which grows with the repo (~300 MB on a 6 TB one).
- **Load time** (index load + listing): SMB before 3.50, 3.84, 5.69, 6.21, 6.71, 6.96 s; after 4.67, 6.06, 6.46 s (the
  NAS was slower this day than on 2026-10-20). Warm mount before 0.79, 0.79, 0.93, 0.95 s; after 1.03, 1.06 s: the cost
  of decoding on four threads instead of 16. The first mount run after SMB runs is colder (2.6–3.9 s either way) and
  isn't comparable.
- **rayon's global pool during the load** (a sampler queues an empty job every 10 ms and books its wait to start):
  before 130–313 ms longest wait in every run, the pool's 16 workers all inside rustic's load; after 0.2–12.5 ms.
- Unlock 0.46–0.60 s and 43–45 MB of RSS after it, either way.

## Draft #422 comment (not posted)

> Measured the open cost on my NAS repo (9 snapshots, 2.4M blobs, 132 GiB, 112 MB of index files; rustic_core 0.13.0, M3
> Max, NAS over Tailscale on the LAN).
>
> - Unlock + snapshot list: ~0.5 s, ~45 MB RSS. Browsing the dated folders is instant.
> - Index load: 2.5–4.3 s over a direct SMB session, 1.6–2.1 s cold through the mount. Peak RSS ~0.55–0.7 GB (~5x the
>   index files' size).
> - rustic's cache: the first browse downloads whole tree packs (265 MB for 160 trees) and writes 360 MB to disk. The
>   warm re-open is great (0.25 s), but not worth that. Going with `no_cache`.
> - Copy-out over direct SMB was ~20 MB/s, because every ranged read opened and closed the pack. Holding pack handles
>   open brought it to ~25 MB/s and tree reads from ~14 ms to ~6 ms; reading blobs ahead in parallel is next.
>
> Plan: `to_indexed` up front, one indexed repo at a time, drop the index after five idle minutes. Details in
> `docs/notes/restic/open-cost.md`.
