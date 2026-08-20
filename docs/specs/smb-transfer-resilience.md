# SMB transfers that survive a server going quiet, without giving up throughput

**Status**: M0-M3, M4.1, M4.3, and M4.4 shipped; M4.2's mechanism is in and gated shut, and 0.16.0's keepalive does not
open it. **Owner**: David. **Date**: 2026-08-01.

A 764-file copy to a QNAP NAS wedges permanently, twice reproduced. Everything downstream of that — the frozen dialog,
the dead Rollback, the corrupt files — is a symptom.

> **Correction (2026-08-01).** The credit diagnosis below was real but was **not** the cause of the permanent wedge.
> M0/M1 shipped and were correct; the wedge kept happening on `smb2` 0.14.0 with a longest credit wait of 47 ms. The
> actual cause was on the send side: one `TcpTransport::send` held the write half forever and every later request queued
> behind it, so the client stopped sending and the server's silence was a consequence, not a cause. Fixed in `smb2`
> 0.15.0 (writer task + `Error::SendTimeout` + RAII waiters + `sent_age`). Full account:
> `docs/notes/incidents/2026-07-31-transfer-wedge/README.md` § Resolution. ❌ Don't re-anchor on credits when reading
> the milestones below.

The 2026-07-31 incident record is `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`, and its closing list names
where each piece of the observability and recovery work now lives. That work is what turned an undiagnosable hang into a
20-minute read; the two pieces M4 below builds directly on are the in-flight table and stall watchdog
(`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "The stall signal") and the staging
guarantee that makes abandoning a wedged worker safe (same file, § "File writes are staged").

Read before starting: `apps/desktop/src-tauri/src/file_system/volume/backends/CLAUDE.md`, the `smb2` crate's `AGENTS.md`
and `docs/releasing.md`, and MS-SMB2 §3.2.4.1.5 / §3.3.1.2 on credits.

## The diagnosis, and the evidence for it

Observed at the wedge: seven copy tasks all at `bytes_done == total_bytes` awaiting a write that never returned; 13
requests outstanding with no response (6 `Write`, 4 `QueryInfo`, 3 `Close`, 1 `Create`), oldest 67 s; **zero** SMB
responses of any kind afterwards (154 `fs_info` requests in the first capture, 0 completions); TCP `ESTABLISHED`
throughout, the NAS answering pings at 5.9 ms, and macOS's own SMB client serving the same share instantly.

In the crate:

- `Inner::credits` starts at 1, is `store`d at negotiate, and afterwards **only ever grows** (`saturating_add` per
  response). Nothing decrements it.
- No send path gates on credits.
- `credit_charge` is computed correctly per request (`payload.div_ceil(65536).max(1)`).
- The one consumer of the counter, `tree.rs`'s pipeline fill, divides that ever-growing number by the per-request charge
  — so the gate is unbounded in practice, and it is applied **per write stream against a per-connection counter**, so N
  concurrent streams each fill their own window from the same number with nothing tracking the total.

**Caveat on confidence.** The mechanism is unambiguous from the code, but the over-spend has never been _observed_:
`smb2::client::connection` logs `dispatch:` only on the `ChangeNotify` path, so `Write` / `Create` / `Close` charges are
invisible. M0 closes that and confirms the reading.

## Settled decisions

1. **Correct credit accounting is not a performance compromise; it is the thing that lets us go fast safely.** The
   current client isn't fast, it's unbounded. A correct one paces to the server's actual stated budget, which on modern
   servers is larger than our current concurrency of 8 exploits. ❌ Don't "fix" this by lowering concurrency — that is
   the only option here that genuinely costs throughput.
2. **Credits alone are not enough.** They fix ONE cause of a silent server. A NAS reboot, Wi-Fi roaming without a TCP
   RST (routine on macOS sleep), a share going offline, or a disk stall all still hang forever. Detection has to be
   cause-independent.
3. **The deadline belongs on "is the session alive", never on "has this write finished".** A large write to a loaded
   spinning-disk NAS is legitimately slow. Timing out the operation trades a rare wedge for frequent spurious failures,
   which is worse. ECHO is what separates the two.
4. **A credit gate must not become a starvation hang.** If the server goes quiet, credits never replenish, and a naive
   "wait for credits" reproduces the original bug from the client side. Every wait is bounded and surfaces a typed
   error.
5. **Detection is the floor, not the goal.** Erroring cleanly beats hanging; surviving the blip is the actual target.
6. **This class of bug is only reachable by fault injection.** It survived because nothing ever simulated a
   hostile-but-plausible server. Tests that grant a small window and go silent are part of the fix, not follow-up.

## M0: see the over-spend (in `smb2`)

Finishes the half of the earlier `M1.3` that was left undone.

- **M0.1** Log `dispatch:` for every command, not just `ChangeNotify`, with `credit_charge` and the credits believed
  available at send time.
- **M0.2** A test that demonstrates over-spend against the CURRENT code: a transport double granting a small window,
  asserting the client sends beyond it. This is the red step for M1 and the confirmation of the diagnosis above.

## M1: real credit accounting (in `smb2`)

- **M1.1** Spend on send: decrement by `credit_charge` when a request goes out.
- **M1.2** Gate when short: if available < charge, wait for replenishment instead of sending anyway — **bounded**, per
  Decision 4, surfacing a typed error rather than blocking forever.
- **M1.3** Make the budget **connection-wide**, so concurrent streams share one pool instead of each independently
  reading the same counter.
- **M1.4** Audit what we request: `header.credits = 256` appears at several send sites unexamined, and a client must
  keep a credit in reserve to stay able to send at all.
- **M1.5** Fault-injection tests: a server that grants a small window and goes silent on over-spend; one that stops
  granting (the starvation case from Decision 4).

## M2: detect a dead session, whatever killed it (in `smb2`)

- **M2.1** A response deadline that fires on silence, producing a typed error to every waiter.
- **M2.2** **SMB2 ECHO keepalive**, so "slow but alive" is distinguishable from "dead" (Decision 3). This is what makes
  M2.1 safe to set aggressively, so it is not optional garnish.
- **M2.3** Tests: a server that dies mid-write, and one that drops TCP without a RST.

## M3: survive it, don't just report it (in `smb2`)

✅ **Shipped in `smb2` 0.16.0, and deliberately not consumed by Cmdr yet.**

- **M3.1** ✅ `ClientConfig::auto_reconnect` now arms a reviver bounded by `ReconnectPolicy` (4 attempts, 0.5 s → 8 s
  backoff, 60 s total). The revival happens IN PLACE, so `Connection` clones held by `FileWriter` / `Watcher` / the
  pipeline stay usable. Only work whose retry can't change what you asked for is replayed; a write, delete, or rename
  that died in flight surfaces the error instead.
- **M3.2** ✅ Durable handles v2 (`open_file_durable` / `reclaim_durable_handle`), a reclaim gated on two independent
  proofs. SMB 2.1's v1 handles are deliberately not implemented (they identify an open by a recyclable server id).
- **M3.3** ✅ Verified end to end against Samba 4.20.6.
- **Open for David**: Cmdr sets `auto_reconnect: false` on all four `ClientConfig` sites and runs its OWN reconnect
  state machine (`smb/reconnect.rs`, single-flight, credential-refreshing, watcher- and index-aware). Turning the
  crate's on would mean two reconnect layers, which `backends/CLAUDE.md` explicitly forbids ("❌ No second reconnect
  loop"). Consuming M3 is its own effort: decide which layer owns recovery, and whether a survived blip stays silent
  (Open question 2).

## M4: Cmdr picks up the pieces

- **M4.1** ✅ **Shipped** (`transfer/retry.rs`, inside `stream_pipe_file`). A transport blip re-runs the FILE from its
  first byte — 3 attempts, 250 ms then 1 s of cancel-aware backoff, a fresh staging temp each time — and the transfer
  carries on. Retryability is an exhaustive typed `VolumeError` match; ❌ a cancel is never retryable and always wins.
  Why that layer and how each data-safety invariant survives it: `transfer/DETAILS.md` § "Retrying one FILE". **Open for
  David**: a file that exhausts its attempts still ends the operation. Carrying on past it needs a "finished, N files
  missing" terminal shape and a product call; deliberately not guessed at.
- **M4.2** 🔒 **Mechanism shipped, teeth gated, and 0.16.0's keepalive does NOT open the gate.** The watchdog can end a
  task's wait (turning a wedged park into a typed error M4.1 retries), but only on
  `Volume::connection_liveness() == Dead` AND 180 s of zero byte movement. **No backend answers that**, so in production
  it still only reports: dumps the in-flight table, feeds the UI's stall signal, acts on nothing. ❌ Elapsed silence is
  not allowed to stand in for the verdict — that is Decision 3, and doing it here would reintroduce one layer up the
  exact failure mode M2.2 exists to prevent. Checked and rejected as liveness signals, in 0.15.0: `sent_age` (restates
  the ambiguity), `send_queue_depth` / `send_failures` / `wire_bytes_sent` (client-side), `disconnected` (a consequence
  the retry already handles); and again on 0.16.0 (2026-08-02): `keepalive_failures` / `keepalive_probes_skipped` (by
  design NOT death — measured against David's QNAP TS-464, an ECHO probe under heavy write load reported
  `2 answered, 1 unanswered` while five consecutive idle runs reported `0 unanswered`), and `Error::ServerUnresponsive`
  (sound, but an error handed to the caller AFTER tearing the connection down, so every waiter — including the parked
  task the watchdog would unstick — has already been failed; the M4.1 retry has it). **Flip-on** now needs a change in
  `smb2` first: expose the conjunction it already computes internally (`unresponsive_for()`: keepalive armed AND the
  wire silent past the liveness window with a request outstanding) as pollable state, readable BEFORE a request burns
  its deadline and WITHOUT the connection being torn down. Then override `connection_liveness` on `SmbVolume` alone.
  Nothing else moves. ❌ And do NOT then drop the stillness window and trust the verdict: the keepalive is least
  trustworthy exactly when a transfer is running, so the AND is load-bearing and the 180 s debounce is doing real work
  rather than just waiting. Full reasoning and the guard tests: `transfer/DETAILS.md` § "The watchdog ACTS".
- **M4.3** ✅ **Shipped, but not as specified.** The premise ("this is where the throughput upside sits") did not
  survive measurement: swept 1-32 against a real QNAP, the window is worth ~14% at best on many-small and nothing on
  few-large, because **74% of the fastest run was a serialized per-file destination probe no window width can overlap**
  (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`). So the window formula was NOT replaced with a credit
  budget. Two things shipped instead: (1) a LOCAL volume's `max_concurrent_ops` no longer bounds a REMOTE peer, which is
  a defect fix — `LocalPosixVolume`'s CPU heuristic won the `min()` on every Mac, so `network.smbConcurrency` did
  nothing above 4-8 while advertising 1-32 (worth 25% on an 8-core Mac, nothing on a 16-core one); (2) the per-file
  destination probe is skipped for a destination directory the operation itself created. ❌ The 32 ceiling stays: the
  NAS plateaus at 12 on both shapes. Open question 3 below is answered by the same numbers — "let the credit budget
  decide" has no well-defined file-level value, since credits gate WRITE frames connection-wide while the window gates
  concurrent FILES. Decisions and guardrails: `transfer/DETAILS.md` § Key decisions.
- **M4.4** ✅ **Shipped** (`smb_full_concurrency_test.rs`). 400 local sources onto the Docker share at the driver's own
  concurrency, sized onto BOTH SMB write paths off the session's negotiated `max_write`, every byte verified, and the
  window's real peak fill asserted so a batch that quietly went sequential can't pass. Its wait is bounded and prints
  `transfer_probe`'s live in-flight table on expiry instead of hanging; a sibling test parks a copy on purpose to prove
  that bound fires and that the dump names the phase. What it covers and what it does not:
  `apps/desktop/src-tauri/src/file_system/volume/backends/DETAILS.md` § Testing.

## Sequencing

M0 and M1 together (M0 is M1's red step), released as one `smb2` version. M2 next, with M3 either alongside or as its
own release — `smb2-credits` argues that call. M4 last, since M4.1 and M4.2 depend on M2's typed error existing and M4.3
depends on M1's budget being real.

## Open questions for David

1. **How aggressive should the session deadline be** once ECHO can tell alive from slow? Seconds, not minutes, is the
   point of ECHO, but the floor is a product call.
2. **Should a reconnect be silent?** A transfer that survives a NAS bounce without the user noticing is the nicest
   outcome, but silently papering over a flaky network hides a real problem worth seeing.
3. **M4.3's replacement**: let the credit budget alone decide, or keep a sanity ceiling? A pure budget is elegant and
   self-tuning; a ceiling is one number to explain but bounds memory when a server grants extravagantly.
