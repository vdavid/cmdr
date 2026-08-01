# SMB transfers that survive a server going quiet, without giving up throughput

**Status**: specced, M0 in flight. **Owner**: David. **Date**: 2026-08-01.

A 764-file copy to a QNAP NAS wedges permanently, twice reproduced. Everything downstream of that — the frozen dialog,
the dead Rollback, the corrupt files — is a symptom.

> **Correction (2026-08-01).** The credit diagnosis below was real but was **not** the cause of the permanent wedge.
> M0/M1 shipped and were correct; the wedge kept happening on `smb2` 0.14.0 with a longest credit wait of 47 ms. The
> actual cause was on the send side: one `TcpTransport::send` held the write half forever and every later request queued
> behind it, so the client stopped sending and the server's silence was a consequence, not a cause. Fixed in `smb2`
> 0.15.0 (writer task + `Error::SendTimeout` + RAII waiters + `sent_age`). Full account:
> `docs/notes/incidents/2026-07-31-transfer-wedge/README.md` § Resolution. ❌ Don't re-anchor on credits when reading
> the milestones below.

The 2026-07-31 incident record is `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`; the observability and
recovery work that made this diagnosable shipped as `transfer-wedge-observability.md` (M1-M6, all merged) and is what
turned an undiagnosable hang into a 20-minute read.

Read before starting: `apps/desktop/src-tauri/src/file_system/volume/backends/smb/CLAUDE.md`, the `smb2` crate's
`AGENTS.md` and `docs/releasing.md`, and MS-SMB2 §3.2.4.1.5 / §3.3.1.2 on credits.

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

- **M3.1** Implement `ClientConfig::auto_reconnect`. It exists today and does nothing — its own doc says the logic "will
  be implemented alongside the concurrent pipeline", and the flag is merely stored.
- **M3.2** SMB2.1 durable / SMB3 persistent handles, so an interrupted write resumes rather than restarting.
- **M3.3** Tests: a transfer that survives a server bounce mid-file.

## M4: Cmdr picks up the pieces

- **M4.1** Retry the FILE, not the operation. Today one failed write kills the whole transfer; once a dead session
  surfaces as a typed error instead of a hang, the file should re-run and the transfer continue.
- **M4.2** Have the stall watchdog ACT rather than only report. It currently dumps the in-flight table and the dialog
  says "stalled"; with M2 available it can convert that into a real error and a retry.
- **M4.3** Delete the concurrency guess. `min(src.max_concurrent_ops, dst.max_concurrent_ops, 32)` is a magic number
  standing in for backpressure; with a real credit budget the gate IS the backpressure, self-tuning per connection. This
  is where the throughput upside sits.
- **M4.4** An E2E copying many files at full concurrency to the Docker SMB share, which is the shape that wedges.

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
