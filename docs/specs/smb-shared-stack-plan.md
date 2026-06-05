# Shared SMB stack: adopt-or-start + PID-lease refcounting

Make the `smb-consumer` Docker fixture stack a machine-level shared resource so concurrent agent sessions across git
worktrees stop killing each other's containers. Today every session brings the stack up under the same `-p smb-consumer`
project + fixed host ports, and any session's teardown (`stop.sh`'s `down`, the orchestrator's deferred `Stop`,
`e2e-linux.sh:316`'s conditional `down`) nukes the shared stack out from under a live suite in another worktree —
observed repeatedly. The fix: a machine-wide lock guards an **adopt-or-start** bring-up and a **refcounted, lock-held
teardown** so the stack only goes down when the last user leaves.

This document captures the **intention** behind each decision so the implementing agent can adapt details when reality
pushes back, as long as the intentions stay intact.

## Loud rules

- ❌ **Teardown re-verifies the lease count under the lock and only downs at zero.** Any inconsistency → log + leave the
  stack UP. The failure mode we degrade to is **never-auto-down** (a leaked stack a human reaps), never an auto-teardown
  that races a live suite. This asymmetry is the whole point — re-read it before changing the release path.
- ❌ **Sweep dead-PID leases ONLY on acquire, never on a timer.** A background reaper would race a just-started suite
  whose lease file exists but whose process hasn't yet been observed alive. Sweeping synchronously under the acquire
  lock is the only safe point.
- ❌ **The lock is HELD ACROSS the `compose down`.** Releasing the lock before the down reopens the exact teardown race
  we're closing: an arriving acquirer would see zero leases, start a fresh `up` while the old `down` is mid-flight, and
  get half-torn-down containers. Acquire → re-verify zero → down → release, all inside one held lock. An acquirer that
  arrives during the down blocks on the lock, then starts clean.
- ❌ **No `flock(1)` — it's absent on stock macOS** (confirmed: `which flock` → not found; `shlock` exists but is a
  create-or-fail PID lock, not a hold-across-section mutex). The lock primitive is Go's `syscall.Flock(fd, LOCK_EX)`,
  which works natively on both Darwin and Linux. The lease/lock logic therefore lives in **Go**, not bash — see M2.
- ❌ **Do NOT touch the smb2 crate.** `smb2::testing` port resolution and `guest_port()` stay as-is. If the audit (M1)
  surfaces a fix that needs a crate change, document it as a follow-up for David; it's out of scope here.
- ❌ **Do NOT edit the vendored compose files** under `.compose/` (byte-for-byte vendored from smb2; see
  `.compose/VENDORED.md`). The `docker-compose.override.yml` from the quick-wins work is the cmdr-owned, re-vendor-safe
  layer; keep using it.
- ❌ **No `git push`, no commit** until David approves.
- Don't run `cargo update` / `go get -u`; no dep bumps are needed (Go stdlib `syscall` only).

## Fresh investigation findings (verified in this worktree)

1. **One bring-up entry, three teardown surfaces.** Every bring-up flows through `start.sh` (orchestrator shells out;
   `e2e-shared/smb-fixtures.ts:102` shells out; `e2e-linux.sh:317,321` shells out; both workflows call it). Teardown has
   three independent surfaces that each issue a raw `down`: (a) `stop.sh:15` (`compose … down`), called by the
   orchestrator's `Stop()` (`smb_orchestrator.go:95`), by the `smb-soak` workflow, and available to humans; (b)
   `e2e-linux.sh:316` — a conditional `down` on the "running but not serving" restart path; (c) the orchestrator's
   `Stop()` itself, which just calls `stop.sh`. All three must route through the new lease-release.
2. **Ports are fixed and env-driven, shared across worktrees.** `smb_ports.go::ApplySmbPortEnv` pins cmdr's stack to
   `SMB_CONSUMER_*_PORT = 11480–94` in the process env; `smb2::testing::*_port()` reads the same vars
   (`smb2/src/testing/mod.rs:129+`). Two worktrees both resolve to 11480+ under the same `-p smb-consumer` project — so
   a second `up` from a branch with different config _recreates_ the running containers (the `(a)` failure), and either
   one's `down` kills the shared set (the `(b)` failure). Fixed ports are intentional (disjoint from smb2's 10480+) and
   stay; the lock is what makes the shared project safe.
3. **CI is single-session per job — the lease mechanism must be harmless there.** `ci.yml` runs
   `desktop-rust-integration-tests` (job at `:118`) and `desktop-e2e-linux` (`:194`) as **separate jobs on separate
   runners**; `slow-checks.yml`'s `smb-soak` is a third isolated job. Each job is one session that starts and stops its
   own stack. The lease refcount naturally no-ops to "one lease in, one lease out, down-at-zero" there — correct with
   zero special-casing. Confirm (don't assume) that a single-PID acquire→release cycle downs cleanly.
4. **The orchestrator already centralizes lifecycle within one `check.sh` run** (`smb_orchestrator.go`): start once,
   `defer Stop()` once, mutex-guarded `startedModes`. That solved _intra-process_ contention; it does nothing for
   _cross-process / cross-worktree_ contention because two `check.sh` processes have independent orchestrators. The
   machine-wide lease is the cross-process layer the orchestrator's in-process mutex can't provide.
5. **Healthchecks already exist to gate adoption on.** The quick-wins override (`.compose/docker-compose.override.yml`)
   added `restart: unless-stopped` + `mem_limit` + `cpus`, and every non-flaky consumer Dockerfile bakes
   `HEALTHCHECK … nc -z localhost 445`. So adopt-or-start can gate "is the project already serving?" on
   `docker compose ps`'s health column, not just a TCP probe. `smb-consumer-flaky` has no healthcheck by design —
   adoption logic must not require it healthy.
6. **Lock primitive: Go `syscall.Flock`.** `flock(1)` is absent on macOS; `/usr/bin/shlock` is create-or-fail, wrong
   shape. Homebrew bash 5.3 is present (start.sh's `mapfile` needs ≥4) but ships no `flock`. Go 1.25.11 is on PATH and
   `syscall.Flock(int(f.Fd()), syscall.LOCK_EX)` is portable Darwin+Linux. **Decision: a small Go lease helper, invoked
   both in-process by the orchestrator and as a subcommand by the bash scripts.** Bash stays thin (it calls the helper;
   no lock logic in bash).
7. **Lease directory location.** `$TMPDIR` on macOS is per-user (`/tmp/claude-501/…`), which would _defeat_ a
   machine-wide lease if two users ran concurrently — but the contending sessions here are all David's worktrees under
   one user, and cross-user SMB-stack sharing isn't a goal. Use a **fixed, user-stable** path: `/tmp/cmdr-smb-leases/`
   for the lease files and `/tmp/cmdr-smb.lock` for the flock file (both world-traversable, predictable, survive across
   worktrees). `/tmp` (not `$TMPDIR`) precisely because we want one shared namespace, not a per-shell one. The lock file
   is separate from the lease dir so flock targets a stable inode.

## Audit table feeds M1 (concurrency safety)

The fix must not just stop the teardown races — it must confirm that two suites genuinely sharing the _same running
containers_ don't corrupt each other's data or timing. M1 produces this table; only what it flags red gets a code change
in M3.

## M1 — Concurrency-safety audit (no code change)

### Scope

Enumerate every SMB-writing and timing-sensitive test path and classify each as **concurrent-safe as-is** / **needs
per-run namespacing** / **needs exclusivity**. Produce the table below directly in this plan (or a `docs/notes/`
companion linked here). No code changes in M1 — it scopes M3.

**Write paths:**

- **cmdr Rust integration tests** (`smb_integration_test.rs`, the `-E 'test(smb_integration_)'` lane): every test names
  its scratch dir via `smb_test_support.rs::test_dir_name()` → `cmdr-test-{pid}-{ts}-{n}` (PID + nanos + process-atomic
  counter). **Already cross-process unique → concurrent-safe as-is.** Confirm no integration test writes to a fixed path
  outside `test_dir_name()`.
- **cmdr Rust soak test** (`smb_soak_test.rs::smb_soak_copy_loop`): `#[ignore]`, _not_ `smb_integration_`-prefixed, so
  the check lane never runs it; manual/CI-`smb-soak`-job only, single-session. Uses `test_dir_name()` for its source dir
  → write-safe. Its assertions are **relative drift** (last-10% avg vs first-10% avg, `< 1.20×`) plus RSS/FD deltas,
  _not_ absolute throughput — uniform concurrent slowdown doesn't trip it, but a concurrent run that _ramps_ load
  mid-soak could inflate drift. Classify **timing-sensitive but out-of-lane** → no lane fix needed; note that a human
  running the soak manually should take the exclusivity lock (M3) if other SMB work is live.
- **Playwright E2E cross-storage copy** (`smb.spec.ts`): writes into `SMB_E2E_SUITE_DIR = 'e2e-playwright'`
  (`e2e-shared/smb-fixtures.ts:64`) — a **fixed** subdir, deliberately disjoint from the Rust tests' `cmdr-test-*`
  space, so E2E ↔ integration sharing is safe. But the name is fixed, so **two concurrent E2E sessions would collide**
  in that subdir. On Linux CI this is impossible (single job, Docker-internal :445); on macOS two Playwright worktrees
  sharing the host stack _could_ collide. Classify **needs per-run namespacing IF macOS multi-E2E is in scope** — verify
  whether that's a real workflow before fixing; likely defer.
- **smb2 harness consumers** (`smb2::testing`): out of scope by rule (no crate edits). The harness's own tests run under
  smb2's `consumer` project on 10480+, a _different_ project/port set — they don't share cmdr's `smb-consumer`
  containers at all. Note as **not applicable** (different stack).

**Timing-sensitive / special containers:**

- **`smb-consumer-slow`** (netem-delayed): used by `virtual_smb_hosts.rs` and referenced in
  `smb_integration_test.rs`/backends `CLAUDE.md`. Check whether any _lane_ test asserts on absolute latency through it
  (vs just functional behavior over a slow link). If functional-only → concurrent-safe. If any throughput/latency
  _assertion_ exists → **needs exclusivity**.
- **`smb-consumer-flaky`** (cycles up/down by design): any test driving it asserts reconnect behavior, not timing.
  Concurrent reads from a second suite during its cycle could see it down at an unexpected moment. Classify whether the
  flaky-using tests tolerate an _externally-induced_ extra down-window; if not → **needs exclusivity** for the
  flaky-using group.

**Deliverable — the table** (fill during M1, one row per path):
`path | writes-to | per-run-unique? | timing-assert? | verdict (safe / namespace / exclusive)`.

### Intentions

- Prove the shared-containers model is data-safe _before_ writing the sharing code, so M3 fixes exactly what's broken
  and nothing more.
- Bias toward "already safe": the integration tests' `test_dir_name()` and the E2E suite-dir isolation were built for
  exactly this — the audit's job is to confirm coverage and find the gaps, not re-namespace everything.

### Test plan

- Read-only: grep every `smb_integration_*` test for write/create/delete targets; confirm each derives from
  `test_dir_name()`. Grep E2E specs for SMB write targets; confirm each uses `SMB_E2E_SUITE_DIR`. Grep all SMB tests for
  latency/throughput `assert!`s.
- No execution needed; this milestone is analysis.

### DONE

The classification table is in this plan (or linked `docs/notes/` file), every write/timing path has a verdict, and M3's
scope is exactly the set of red rows (expected: empty or near-empty for the lane; macOS-multi-E2E and manual-soak noted
as conditional/deferred).

## M2 — Lease/lock mechanism + adopt-or-start + teardown rerouting

### Scope

**A Go lease helper** (a dedicated `scripts/check/smblease/` lib package + a thin `scripts/check/smb-lease/` CLI main —
see "The Go helper is a dedicated package" below). It owns the lock and the refcount; bash shells out to the CLI, the
orchestrator imports the lib.

Lock + lease primitives:

- `/tmp/cmdr-smb.lock` — the flock target. Acquire with `syscall.Flock(fd, LOCK_EX)`; hold for the full
  acquire-or-release critical section; release by closing the fd.
- `/tmp/cmdr-smb-leases/<holder-id>` — one file per live holder. File content can carry the worktree path + a config
  hash for diagnostics.

**Holder model — acquire takes a `holder-id`, NOT always `self-pid`.** A naive `<self-pid>` lease breaks every
standalone caller: `start.sh` exits seconds after the `up`, so its PID is dead by the next acquire and the sweep reaps
it — downing the stack under a live session. Worse on `e2e-linux.sh`, where the multi-minute test phase (the
`docker run`) is a _different_ process from the `start.sh` that did the bring-up. So the holder-id is explicit:

- **`start.sh` (manual / default)** → a **sentinel `manual` lease** the dead-PID sweep NEVER reaps; only `stop.sh` (or a
  helper `--force`) removes it. A forgotten `manual` lease means the stack lingers — the benign direction (a human reaps
  it), never a teardown under a live run.
- **`e2e-linux.sh`** → its **own** holder-id bound to the long-lived harness shell's PID (the `e2e-linux.sh` process
  itself spans the whole run; it already installs a `trap … EXIT` at `:475`). It acquires at `start_smb_containers` and
  releases on EXIT — independent of `start.sh`'s internals (the inner `start.sh` it shells out to runs as the `manual`
  holder, which is harmless: a second healthy-stack acquire just adopts).
- **The orchestrator** → its own `check.sh` PID (long-lived for the whole run).

Acquire is **idempotent per holder-id**: re-acquiring with a holder-id that already has a lease is a no-op rewrite, not
a second refcount. This lets `e2e-linux.sh`'s own acquire and the child `start.sh`'s `manual` acquire coexist as two
distinct holders without double-counting.

**Acquire (bring-up), under the held lock:**

1. Sweep dead-PID leases: for each **numeric** `<pid>` file, if `kill(pid, 0)` says the process is gone, remove the
   file. (Only here — never on a timer.) **The `manual` sentinel is non-numeric, so it's never swept.**
2. Write own lease `<holder-id>` (the caller-supplied id; `manual` for bare `start.sh`).
3. Inspect the running project: `docker compose -p smb-consumer ps` for the requested services.
   - **All requested services running + healthy + config-hash label matches** → **adopt**: no compose call at all
     (avoids the recreate). Return success.
   - **Partially up / unhealthy** → `up -d` reconcile (idempotent; brings missing/sick ones up without disturbing
     healthy ones).
   - **Config-hash differs** → apply the **adopt-vs-reconcile policy table** below (the crux: never recreate under a
     foreign live lease).
4. Release the lock. (The TCP/health probe loop in `start.sh` runs _after_ the lock is released — adoption still
   verifies serving, it just doesn't hold the lock across the probe.)

**Adopt-vs-reconcile policy table** (evaluated under the held lock, _after_ writing own lease so "other leases" excludes
self):

| State                               | Other live leases?         | Action                                                                                                                                                                                        |
| ----------------------------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| All services healthy + hash matches | any                        | **Adopt** — no compose call                                                                                                                                                                   |
| Hash mismatch                       | **yes** (a foreign holder) | **Adopt anyway** + WARN loudly. The running config is the first-comer's; tests run against slightly-stale fixture config — benign vs killing a live run. NEVER `up -d --force-recreate` here. |
| Hash mismatch                       | **no** (only self)         | **Reconcile** via `up -d` — safe, nobody else is using the stack.                                                                                                                             |
| Partially up / unhealthy            | any                        | `up -d` reconcile (brings missing/sick services up; doesn't disturb healthy ones)                                                                                                             |

The mismatch-under-foreign-lease row is the original disease: "WARN + reconcile" would recreate containers under a
sibling's live suite. Adopt-anyway trades a stale-fixture risk (benign — the audit confirms fixtures are baked into the
image, identical across config drift; only ports/limits could differ, which a live adopter inherits fine) for never
re-breaking the live run.

Config-hash: hash the merged compose inputs (the vendored `docker-compose.yml` + the override + the resolved service
set + the `SMB_CONSUMER_*_PORT` env). Stamp it as a compose label on `up` so a later adopter can compare. If labeling
the running stack is awkward, fall back to hashing the same inputs at adopt-time and comparing to a hash file written
next to the lock — pick the simpler reliable form.

**Release (teardown), under the held lock:**

1. Remove own lease `<holder-id>` (the same id passed at acquire; `manual` for bare `stop.sh`).
2. Re-verify the lease count under the lock. If **zero** leases remain → `docker compose -p smb-consumer down`, **with
   the lock still held** (an arriving acquirer blocks until the down finishes, then starts fresh).
3. If **>0** leases remain → do nothing (another session still needs the stack).
4. **Any inconsistency** (lease dir unreadable, ps disagrees, down errors) → log + leave the stack UP. Never down on
   uncertainty.
5. Release the lock.

**PID-reuse note (accepted by design):** the dead-PID sweep uses `kill(pid, 0)`, which can't tell a recycled PID from
the original holder. If the OS reuses a dead holder's PID for an unrelated process, that lease reads as "alive" and
won't be swept → the refcount stays stuck-high → the stack lingers a bit longer than strictly needed. This is the benign
direction (linger, never premature teardown), so it's acceptable; a forgotten lease costs one manual `stop.sh` or
`--force`.

**Optional linger debounce (~60 s):** on reaching zero leases, instead of downing immediately, schedule the down and
cancel it if a new acquire arrives within the window — saves a churny down/up when suites run back-to-back. **Include
only if it stays simple** (a held-lock timestamp check on next acquire, not a background timer). Otherwise note as a
future improvement and down immediately at zero.

**Wiring (the CLI seam — how each caller reaches the helper):**

- **`start.sh`** (holder `manual`): before the `compose up`, call `acquire manual` (which may adopt → start.sh then
  skips its own `up` and goes straight to the probe, or reconcile → start.sh's existing `up` runs). Cleanest shape: the
  helper _decides_ and start.sh acts on its exit signal (e.g. exit 0 = "adopted, skip up"; exit 10 = "reconcile, run
  up"). Keep start.sh thin — it must not reimplement any lock logic.
- **`stop.sh`** (holder `manual`): replace the raw `docker compose … down` with `release manual`.
- **`smb_orchestrator.go::EnsureStarted` / `Stop`** (holder = `check.sh` PID): call the Go helper **in-process** (same
  package — no subprocess), so a `check.sh` run takes/releases its lease directly. `Stop()`'s current `./stop.sh`
  shell-out becomes a direct `release()` call.
- **`e2e-linux.sh` — acquire/release the WHOLE flow, not just `:316`.** This is the orphan path: CI runs `e2e-linux.sh`
  **directly** (`ci.yml:229`, `working-directory: ./apps/desktop`), never through `check.sh`, so the orchestrator's
  lease never exists for the e2e-linux job. The script must own its lease itself:
  - At `start_smb_containers` (its own holder-id = the `e2e-linux.sh` shell PID, `$$`), call `acquire $$ e2e` **before**
    the existing running/probe logic, and set `SMB_LEASE_HELD=1` once it succeeds. The bare `start.sh` it shells out to
    (`:317,321`) still runs as `manual` — harmless second holder.
  - **Replace ALL existing `trap … EXIT` sites with one early consolidated `cleanup()`.** The script today has THREE
    single-handler `trap … EXIT` (`:131` and `:221` restore `.cargo/config.toml`; `:475` kills `APP_PID`), each
    overwriting the previous — last-trap-wins, tolerated only because the branches are sequential. Adding a fourth bare
    `trap` for the lease would silently drop one of the others. Consolidate into one guarded function installed ONCE at
    the top of the script (right after `set -e`, before any branch), so a single early `trap cleanup EXIT` covers every
    concern regardless of which branch ran:

    ```bash
    cleanup() {
        # App: kill the backgrounded Tauri app if it was launched.
        [[ -n "${APP_PID:-}" ]] && { kill "$APP_PID" 2>/dev/null; wait "$APP_PID" 2>/dev/null || true; }
        # SMB lease: release only if we acquired one (never down a stack we don't hold).
        [[ -n "${SMB_LEASE_HELD:-}" ]] && (cd "$REPO_ROOT/scripts/check" && go run ./smb-lease release "$$" 2>/dev/null || true)
        # Cargo config: restore the temporarily-cleared dev override if a backup exists.
        [[ -n "${CARGO_CONFIG_BAK:-}" && -f "$CARGO_CONFIG_BAK" ]] && mv "$CARGO_CONFIG_BAK" "$CARGO_CONFIG" 2>/dev/null || true
    }
    trap cleanup EXIT
    ```

    Each clause is runtime-guarded on a variable that's empty until its resource exists, so the single early trap is
    safe in every branch (the guards no-op the irrelevant clauses). **Delete the three existing `trap … EXIT` lines at
    `:131`, `:221`, and `:475`**; keep their backup-variable assignments (`CARGO_CONFIG_BAK=…`, `APP_PID=$!`) — the
    consolidated `cleanup()` reads those. Note `:221`'s branch uses a literal `${CARGO_CONFIG}.docker-bak` rather than a
    `CARGO_CONFIG_BAK` var; set `CARGO_CONFIG_BAK="${CARGO_CONFIG}.docker-bak"` there too so `cleanup()` sees it
    uniformly.

  - **`:316`'s conditional `down`** (the "running but not serving" restart path) becomes a helper **`reconcile`** verb
    that respects other leases: it must NOT unconditionally `down` the shared stack just because _this_ session sees it
    unhealthy. `reconcile` brings the sick services back under the lock (`up -d` for the unhealthy subset, or
    `--force-recreate` only those services) — never a blanket `down`. If other leases are live, a sick-but-shared stack
    is the first-comer's to manage; log and let the standard probe retry.

**No raw `down` may remain** outside the helper's release path. Grep after wiring: `compose .* down` should appear only
inside the helper (and the historical-comment references in `start.sh`).

**The Go helper is a dedicated package: `scripts/check/smb-lease/`, split lib + thin main.** The check launcher runs
`cd check && go run *.go` (`scripts/check.sh:7-9`), which globs only the top-level `.go` files of `package main` in
`scripts/check/` — it does NOT compile subdirectories, so a verbs-CLI bolted into the runner would have to be hand-wired
into its flag parser. Avoid that entirely: put the helper in its own subpackage under the existing module
`cmdr/scripts/check` (confirmed: `scripts/check/go.mod` declares `module cmdr/scripts/check`, and `checks/` already
lives there as a sibling subpackage — one module, no nested `go.mod`). Structure:

- `scripts/check/smblease/` — a **library** package (`package smblease`) exporting `Acquire(holderID, mode)`,
  `Release(holderID)`, `Reconcile(mode)`, `Status()`. All lock/lease/compose logic lives here.
- `scripts/check/smb-lease/` — a **thin `package main`** that parses one verb (`acquire <holder-id> <mode>` |
  `release <holder-id>` | `reconcile <mode>` | `status`) and calls the matching `smblease.*` function.

**The orchestrator imports the lib directly** — `import "cmdr/scripts/check/smblease"`, calling `smblease.Acquire(...)`
/ `smblease.Release(...)` in-process, no subprocess — **because** it's already `package main` in the same module, so the
direct call shares the lock-path constants and skips a `go run` cold-compile per lifecycle event; only the bash scripts
(which can't import Go) shell out to the thin `main`.

Exact invocations (each bash caller `cd`s into `scripts/check` so `go run ./smb-lease` resolves the subpackage under the
single module; derive `REPO_ROOT` from each script's existing root resolution):

- **`start.sh`**: `(cd "$REPO_ROOT/scripts/check" && go run ./smb-lease acquire manual "$mode")` before its `up`;
  `(cd "$REPO_ROOT/scripts/check" && go run ./smb-lease release manual)` is in `stop.sh`.
- **`e2e-linux.sh`**: `(cd "$REPO_ROOT/scripts/check" && go run ./smb-lease acquire "$$" e2e)` at
  `start_smb_containers`; `release "$$"` in the cleanup trap; `reconcile e2e` for the `:316` path.
- **orchestrator**: `smblease.Acquire(checkPID, mode)` / `smblease.Release(checkPID)` — in-process, no command.

**Use `go run`** (AGENTS.md guarantees `go` on PATH via mise shims). **Fallback if Go is missing** (bash callers only —
the orchestrator can't reach this state, Go already built it): the `go run` exits non-zero, the script logs a loud
warning and **proceeds with the legacy direct `up`/`down`** — never block or hang a manual user on a missing toolchain.
In CI, mise installs Go before any SMB step, so the fallback never fires there; it's purely a local-ergonomics safety
net.

### Intentions

- One lock, one refcount, one teardown decision point — every start/stop in the repo funnels through it.
- Adopt without a compose call when the stack is already serving the right config: that's what kills the
  recreate-mid-run failure.
- Degrade to "leave it up," never to "tear it down," on any doubt.

### Test plan

- `--fast` after wiring (formatters, Go vet/staticcheck, lock-poison/etc.) — the helper is new Go, so it must pass the
  Go static lane.
- Full `./scripts/check.sh` — exercises `desktop-rust-integration-tests` through the new acquire/release on a real
  single-session run (down-at-zero must fire cleanly; the stack must be gone after).
- `./scripts/check.sh --check desktop-e2e-linux` — the e2e lane brings the stack up and tears it down through the
  helper; confirm no orphaned containers and no mid-run recreate.
- Targeted manual: run the acceptance two-session contention test (see Acceptance) by hand against this milestone.

### DONE

The `smblease` lib + `smb-lease` CLI own lock+refcount with explicit holder-ids; `start.sh` (holder `manual`) /
`stop.sh` (holder `manual`) / `smb_orchestrator.go` (holder `check.sh` PID, imports the lib in-process) / `e2e-linux.sh`
(holder `$$`, acquired at `start_smb_containers`, released via the consolidated `cleanup()` EXIT trap, `:316` →
`reconcile`) all route through it; no raw `compose down` survives outside the release path; `--fast` + full
`./scripts/check.sh` + `--check desktop-e2e-linux` green; the contention test passes by hand. Verify the Go-missing
fallback degrades to no-lease + warning, not a hang.

## M3 — Namespacing / exclusivity fixes (only what M1 flagged) + docs

### Scope

Implement **only** the red rows from M1's table.

- **Per-run unique write roots** where M1 found a fixed-name collision _and_ the workflow is real (cmdr-side only;
  smb2-crate fixes are documented as follow-ups, not done here). The likely candidates: the E2E `SMB_E2E_SUITE_DIR`
  (only if macOS multi-E2E is in scope — else defer with a note). Cheap fix: suffix the suite dir with a per-process
  token (PID/worker index) the same way `test_dir_name()` does.
- **One coarse machine-wide advisory exclusivity flock** (`/tmp/cmdr-smb-exclusive.lock`), taken _only_ by the
  timing-sensitive/exclusive test groups M1 flagged (the slow/flaky/throughput groups, if any). Wire it at the cheapest
  seam — investigate: a nextest setup hook? a wrapper the integration-test check takes before launching the exclusive
  subset? the soak script taking it at top? Take the lock for the duration of the exclusive group only, so the common
  (parallel-safe) tests never serialize. Reuse the Go lease helper's flock code for this second lock.
- **Docs**: update `apps/desktop/test/smb-servers/README.md` (the new shared-stack model, lease dir, manual override),
  `scripts/check/CLAUDE.md` (orchestrator now does machine-wide leasing, not just in-process), and
  `docs/tooling/testing.md` (how concurrent runs now coexist; the `manual` sentinel lease and how to force-down a leaked
  or lingering stack: `stop.sh` clears the `manual` lease, or `rm -rf /tmp/cmdr-smb-leases && ./stop.sh` / a helper
  `--force` flag nukes everything). Add a `Decision/Why` to the smb-servers `CLAUDE.md` capturing the holder-id lease
  model + adopt-or-start + lock-held-teardown design and _why_ (the recreate + teardown races, the
  standalone-PID-lifetime hole that drove the `manual` sentinel).

If M1's table is all-green for the lane (the expected outcome given `test_dir_name()` coverage), M3 collapses to **docs
only** + the optional exclusivity lock if any timing assertion exists. State that explicitly at hand-off.

### Intentions

- Fix exactly the audit's red rows — no speculative namespacing.
- Keep the exclusivity lock coarse and narrowly-scoped so it costs nothing on the parallel-safe majority.

### Test plan

- `--fast`, then full `./scripts/check.sh`, then `--check desktop-e2e-linux`.
- If an exclusivity lock was added: run the exclusive group twice concurrently by hand and confirm they serialize (one
  waits for the other) rather than interleave.

### DONE

Every M1 red row has a landed fix or a documented deferral; the three docs are updated; the smb-servers `CLAUDE.md` has
the design `Decision/Why`; gates green.

## Acceptance

1. **Gates green**: full `./scripts/check.sh` plus `--check desktop-e2e-linux`, and one
   `./scripts/check.sh --include-slow` run (exercises e2e-linux + playwright + rust-tests-linux) end-to-end, with the
   stack adopted/started and torn down cleanly.
2. **The deliberate two-session contention test passes.** Script it concretely:
   - **Setup**: a dummy long-lived process holds a lease — start `sleep 600 &`, capture its PID, write
     `/tmp/cmdr-smb-leases/<dummy-pid>` (or have the helper acquire on its behalf), and bring the stack up so it's
     serving.
   - **Run the lane**: in parallel, run the e2e lane (or `--check desktop-rust-integration-tests`) in this worktree. It
     acquires its own lease, **adopts** the already-serving stack (assert: no `--force-recreate`, container IDs
     unchanged across the run — capture `docker compose -p smb-consumer ps -q` before and after), runs green, then
     releases.
   - **Assert survival**: after the lane's teardown, the dummy's lease still exists and the **stack is still up**
     (`docker compose -p smb-consumer ps` shows the services running — the lane's release saw a non-zero refcount and
     did NOT down). This is the core regression the whole plan exists to fix.
   - **Then release the dummy**: kill the dummy, run a `release` for its PID (or let the next acquire's dead-PID sweep
     reap it), and confirm the stack **downs at zero** (`ps` shows no `smb-consumer-*` containers).
   - Capture this as a small shell script under `apps/desktop/test/smb-servers/` (e.g. `contention-check.sh`) so it's
     repeatable, not a one-off.

State to David at hand-off: a single green contention run proves the mechanism is wired; the durable proof is a week of
real concurrent worktree sessions with zero "Cannot reach smb-consumer-\*" cascades.

## Out of scope (tracked, not done here)

- **smb2-crate changes** of any kind. If M1 finds a write/timing fix that needs the crate, it's a documented follow-up
  for David, not work in this plan.
- **Cross-user SMB-stack sharing.** The lease lives in `/tmp` under one user's namespace; two different OS users running
  concurrently isn't a supported scenario (not a real workflow — all contending sessions are David's worktrees).
- **The linger debounce**, if it doesn't stay trivial — note as a future nicety; down-immediately-at-zero is correct and
  simpler.
- **macOS multi-E2E suite-dir namespacing**, unless M1 confirms running two Playwright E2E sessions against the shared
  host stack is a real workflow. Default: defer with a note.
