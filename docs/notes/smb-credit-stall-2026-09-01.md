# The SMB compound read's credit over-charge, 2026-09-01

A 300 GB SMB-to-SMB copy stalled about 30 seconds into its copy phase. **The cause was the compound fast-path charging
SMB credits for `max_read` (8 MB, 130 credits) instead of for the file it was actually reading (4 MB, 66 credits), which
capped the connection at three concurrent reads against a client window of 512 while the transfer launched 10.** Seven
tasks parked waiting for credits that couldn't arrive, and the transfer stopped moving bytes.

Read this before anyone raises `CREDIT_TARGET`, widens the compound fast-path threshold, or adds adaptive concurrency
backoff to the transfer engine. It carries the arithmetic that says which of those is the real lever.

## What the evidence was

The dev log (`~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/cmdr.log`, app 0.41.0, pid 63999,
2026-09-01 12:50 to 13:04). Log files rotate at 50 MB, so the primary evidence is gone; what's below is the extract.

Source `/Volumes/PiHDD` (raspi.local, 192.168.1.150), destination `/Volumes/naspi` (192.168.1.111). Both direct smb2,
SMB 3.1.1, signed, `max_read` 8,388,608 on both. 119,204 files of roughly 4 MB each. Neither connection dropped, and no
error appears anywhere in the run: this was a stall, not a failure.

## The arithmetic

`Tree::read_file_compound` took no size argument, so its READ carried `length: max_read`. Per MS-SMB2 3.2.4.1.2 the
credit charge follows the expected response size, so the charge was `ceil(8 MB / 65536) = 128` for the READ plus one
each for CREATE and CLOSE. `execute_compound` reserves the sum, which is why the warning named the compound by its first
operation:

```
credits: Create needs 130 credit(s) but only 9 are available; waiting for the server to grant more
credits: Create needs 130 credit(s) but only 0 are available; waiting for the server to grant more
```

`CREDIT_TARGET` is 512. 512 ÷ 130 = 3.9, so three concurrent compound reads fit and the fourth onward waited. The stall
dump confirms the prediction exactly: three tasks read their full 4 MB, seven read nothing at all.

```
#0  walking   33644ms                          xiaomi_camera_videos_sorted
#1  streaming 20223ms  4240422/4240422 bytes   read done, waiting on the write side
#2  streaming 18117ms  4417114/4417114 bytes   read done, waiting on the write side
#3  streaming 14365ms  4221846/4221846 bytes   read done, waiting on the write side
#4..#10       4-13s    0/~4.2M bytes           never got a credit
```

Every file in the set qualified for the fast path, because the threshold was "fits in one READ" and 4 MB fits in 8 MB.
So the over-charge applied to all 119,204 of them, not to an edge case.

## Why raising `CREDIT_TARGET` is the wrong lever

The window isn't too small; the charge is too big. A 4 MB read costs 66 credits after the fix and a 100 KB read costs
three, so the same 512-credit window carries seven concurrent 4 MB reads or 170 concurrent 100 KB reads. Raising the
target papers over the over-charge, and servers clamp the request to their own maximum anyway, so it isn't reliably
available.

`CREDIT_TARGET`'s doc comment used to claim 512 was "comfortably more than the deepest pipeline this crate opens (32
requests at 8 credits each for 512 KB chunks)". The compound read violated that stated invariant by a factor of four,
which is the tell that the charge, not the window, had drifted.

## What is NOT explained by this

Two things in the same log that the credit story doesn't account for, both still open:

- **The destination's send side was slow independently.** naspi's socket accepted 425 to 767 KB/s, with 1 MB WRITE
  frames queued 14 to 40 seconds and taking 2.2 to 2.9 seconds each to reach the socket. The three tasks that did read
  their bytes then jammed writing them. Whether that's the Mac's link carrying both halves of the copy, the QNAP being
  busy under a sustained write, or something else isn't answerable from this log. Time a `smb2 put` of a few hundred MB
  to naspi on its own before blaming the transfer engine for it.
- **The source is slow hardware.** The 2026-08-14 run of the same copy averaged 7.4 MB/s over 67 minutes, which reads
  like a Pi-class ceiling. Ten concurrent readers against one USB disk is seek thrash regardless of credits. The credit
  fix removes the self-inflicted half of the problem; it doesn't make the Pi fast.

## The related defects found in the same investigation

- **`read_file_compound`'s truncation guard was load-bearing in a way that's easy to miss.** It compared
  `create_resp.end_of_file > max_read`. Once a caller can request less than `max_read`, that guard has to compare
  against what was actually requested, or a file that grew between the scan and the read comes back at exactly the
  requested length and looks complete. The guard is exact rather than heuristic because the CREATE response carries the
  server's authoritative size in the same compound frame.
- **The stall dump's `driver=` field was useless for this shape of copy.** A single top-level source folder takes the
  sequential driver (`copy.rs`), and `copy_serial.rs` never called `set_driver_phase`, so the dump read
  `driver=starting()` 113 seconds in. A dump from the 2026-08-14 run of this same copy has the identical useless value.
- **`in_flight=11/10` read as a cap violation and wasn't one.** The probe table holds the directory-walker task
  alongside the 10 file slots, while the `/10` counts only file slots.

## The stopgap, if the fix isn't in the build

`Settings > Advanced > Network and mounts > SMB concurrency` (`network.smbConcurrency`, default 10). Set it to three and
the copy fits the credit window unfixed, at a third of the parallelism.
