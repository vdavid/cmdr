# SMB test servers

Docker SMB containers for local development and E2E testing, provided by smb2's consumer test harness.

## Quick start

```bash
./start.sh         # Start core containers (guest, auth, both, readonly, flaky, slow)
./start.sh minimal # Start just guest + auth
./start.sh all     # Start all 14 containers
./stop.sh          # Stop everything
```

The Docker Compose files live in `.compose/`. They're **vendored** from smb2's consumer test harness (see
`.compose/VENDORED.md`). If they're missing or stale after an smb2 bump, follow the re-vendor steps there.

CI runs the Rust SMB integration tests automatically via the `desktop-rust-integration-tests` check, which starts the
`core` containers, runs `cargo nextest run --run-ignored only -E 'test(smb_integration_)'`, and tears them down.
Locally, `pnpm check --rust` includes the same check.

See [docs/guides/testing/smb-servers.md](../../../../docs/guides/testing/smb-servers.md) for the full documentation.

## Shared stack across worktrees (the lease)

The `smb-consumer` stack is a **machine-wide shared resource**. Every bring-up (this `start.sh`, the check-runner's
orchestrator, `e2e-linux.sh`) and every teardown routes through a Go lease helper (`scripts/check/smblease`) so
concurrent sessions in different git worktrees stop tearing each other's containers down. You don't normally interact
with the lease directly — `start.sh` / `stop.sh` handle it — but here's the model:

- **Holder-id leases.** Each live user writes one file under `/tmp/cmdr-smb-leases/<holder-id>`, guarded by a flock on
  `/tmp/cmdr-smb.lock`. Bring-up **adopts** an already-serving stack (no compose call) or **reconciles** it via `up -d`;
  teardown removes the caller's lease and downs the stack **only when zero leases remain**.
- **The `manual` sentinel.** A bare `./start.sh` registers as the holder-id `manual`. It's the one lease the dead-PID
  sweep never reaps (it's non-numeric), because `start.sh` exits seconds after the `up` — a PID-keyed lease would be
  swept on the next acquire and tear the stack down under a live session. Numeric holders (`e2e-linux.sh`'s `$$`, the
  orchestrator's `check.sh` PID) are long-lived processes, so their leases are swept when the process dies.
- **`./stop.sh`** releases the `manual` lease. If another session still holds a lease, the stack stays **up** — running
  `stop.sh` while a sibling worktree's suite is live no longer kills it.

### Force-down a lingering stack

A forgotten `manual` lease (or a leaked numeric one) keeps the stack up. To reap it when you're sure nothing else needs
it:

```bash
rm -rf /tmp/cmdr-smb-leases && ./stop.sh   # clear all leases, then down
# or just confirm the state first:
(cd ../../../../scripts/check && go run ./smb-lease status)
```

`contention-check.sh` in this directory is the repeatable acceptance test for the whole mechanism: a dummy holder must
survive another session's full acquire→run→release cycle, and the stack must down only at zero holders.

### Decision: holder-id leases + adopt-or-start + lock-held teardown

**Why a lease at all.** All worktrees resolve the same `smb-consumer` project on the same fixed host ports, so any one
session's raw `compose down` (from `stop.sh`, the orchestrator's `Stop`, or `e2e-linux.sh`'s restart path) tore the
shared stack out from under a live suite in another worktree, producing `Cannot reach smb-consumer-X` cascades —
observed repeatedly. A second session's `up` with slightly different config could `--force-recreate` the running
containers mid-run. The lease closes both races.

**Why adopt-or-start, not just `up -d`.** When the stack is already serving the requested config, the helper issues **no
compose call** — it adopts. That's what prevents the recreate-mid-run failure. A blind `up -d` from a second session
could disturb healthy containers; adoption never touches them.

**Why the lock is held across the `down`.** Releasing the flock before the `compose down` reopens the exact teardown
race we're closing: an arriving acquirer would see zero leases, start a fresh `up` while the old `down` is mid-flight,
and get half-torn-down containers. Acquire → re-verify zero → down → release all happen inside one held lock.

**Why the `manual` sentinel exists.** A naive `<self-pid>` lease breaks every standalone caller: `start.sh` exits
seconds after its `up`, so its PID is dead by the next acquire and the dead-PID sweep reaps it, downing the stack under
a live session. The non-numeric `manual` holder-id is never swept, so a forgotten `manual` lease lingers — the
**benign** direction (a human reaps it with `stop.sh`), never a teardown under a live run. The whole design degrades to
"leave it UP" on any doubt, never to "tear it down."

See [`scripts/check/smblease`](../../../../scripts/check/smblease/smblease.go) for the full lock/lease/policy model.
