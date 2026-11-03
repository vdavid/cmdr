# What keeping xattrs costs an SMB copy (2026-11-03)

The guardrail bench for #405 (Finder tags, comments, FinderInfo, and resource forks on copies to and from SMB shares):
does a plain file get slower, and what does a tagged one cost? The design it measures: `crates/cmdr-smb/DETAILS.md` §
"Xattrs on copies".

## Rig

- `crates/cmdr-smb/examples/copy_xattrs_bench.rs`, release build, against `smb-consumer-fruit` (Docker, Samba 4.20.6,
  `catia fruit streams_xattr`) on loopback, from an M-series Mac also running other agents' builds, so the machine was
  contended. smb2 0.40.0 (branch `streams-for-copies`).
- Volume-level calls, one file at a time, so a per-file cost shows; the copy engine runs 10 at once.
- "Before" is the read every copy used to make (`open_read_stream_with_hint`); "with the listing" is a copy's read onto
  a destination that keeps xattrs (`open_read_stream_for_copy`). Reads interleave file by file and flip order every run;
  each figure is the median of 9 runs.
- Writes of a plain file take the same path as before by construction (a source with no xattrs sends nothing new), so
  the write rows measure a tagged file's extra cost, not a before and after.

## Results

Two full runs; the spread between them is the noise.

- **Large plain file read (256 MiB)**: 3,147 and 3,078 MB/s before, 3,213 and 3,157 MB/s with the listing. No
  regression: the listing is one extra op in the open's frame.
- **Large plain file write (256 MiB)**: 1,557 and 962 MB/s; the same path as before, so the swing is the machine.
- **1,000 plain small files (4 KiB) read**: 5,268 and 5,577 files/s before, 4,853 and 4,561 files/s with the listing. So
  the listing costs the server about 20–40 µs per open (Samba lists the file's xattrs, and `vfs_fruit` adds its own),
  which is 8–18% of a 0.2 ms loopback read. ❗ Over the 2% bar on loopback. On a LAN, where a round trip is 1–5 ms and
  the local write of each copied file costs more, it's a percent or two at most; unmeasured there.
- **1,000 plain small files written**: 755 and 350 files/s; unchanged path.
- **A tagged file's read**: 0.19 and 0.17 ms without its tags, 0.46 and 0.40 ms with them: about 0.25 ms more on
  loopback, one round trip (the values' read), so a LAN round trip more.
- **A tagged file's write**: 1.86 and 4.29 ms against 1.32 and 2.85 ms for a plain one: about 0.5–1.4 ms more on
  loopback, one overlapped round trip of stream writes.

Earlier, non-interleaved runs of the same bench read the large file 7% slower with the listing in one run and 3% faster
in another: order effects, which is why the reads now interleave.

## What this leaves open

- A plain small file read for a copy that keeps xattrs (SMB to local disk, SMB to SMB) costs the server one listing.
  Cutting it would need to know a file has no streams without asking, which SMB offers only through the AAPL create
  context's `READDIRATTR` on a listing (an smb2 follow-up). A copy onto a destination that keeps nothing of them (SFTP,
  S3, MTP) still lists, for the loss note's sake: one line in `copy_xattrs.rs::XattrAsk::of` would trade that note for
  the listing.
- Not measured: a real NAS (QNAP's `qnap_macea` and Synology), over a LAN.
