# E2E stability quick wins: SMB fixture resilience + Playwright Linux-lane timeouts

Plan for the three approved config-level fixes from the SMB-flakiness investigation: auto-recovering SMB fixture
containers, resource limits so the Docker VM can't evict `smbd` under build pressure, and a looser Playwright Linux-lane
timeout plus one retry. This is a config change, not an architecture program. The structural options from the
investigation — lane serialization, an orchestrator health-gate-retry loop — are explicitly **out of scope**.

This document captures the **intention** behind each decision so the implementing agent can adapt details when reality
pushes back, as long as the intentions stay intact.

## Loud rules

- ❌ **Do NOT edit the vendored compose files in place.** Everything under `apps/desktop/test/smb-servers/.compose/` is
  a byte-for-byte vendored copy of smb2's consumer harness (`.compose/VENDORED.md`), excluded from `oxfmt` to stay
  diff-clean across re-vendors. Editing the Dockerfiles or `docker-compose.yml` directly creates drift that the next
  `rsync --delete` re-vendor silently wipes. Add a **`docker-compose.override.yml`** next to the vendored file instead
  (Compose auto-merges `-f base -f override`); it survives re-vendoring untouched. See M1 for the exact merge wiring.
- ❌ **Do NOT add `restart:` or `mem_limit` to `smb-consumer-flaky`.** Its whole job is to cycle up and down
  (`cycle.sh`, no healthcheck by design). A restart policy would fight the test it exists to serve. The override must
  enumerate services, not blanket-apply.
- ❌ **Do NOT cap the E2E app container tightly.** It runs a full Tauri app + WebDriver + headless chromium under
  `--privileged`; a tight `mem_limit` trades one flake (evicted `smbd`) for another (OOM-killed app, new timeouts).
  Decision in M2 is to leave it uncapped or set only a generous floor — see rationale there.
- ❌ **No `git push`, no commit** until David approves. Config-applied verification only.
- Don't run `cargo update` / `go get -u`; no dep bumps are needed here.

## Fresh investigation findings (verified in this worktree)

1. **Vendoring confirmed.** `apps/desktop/test/smb-servers/.compose/` is vendored from
   `~/projects-git/vdavid/smb2/tests/docker/consumer/` via `rsync -a --delete --exclude=VENDORED.md`. The base
   `docker-compose.yml` (93 lines) declares 15 services with only `build.context` + `ports` — **no restart policy, no
   `mem_limit`, no `healthcheck`**.
2. **Image-level healthchecks already exist.** Every consumer Dockerfile except `flaky` bakes in
   `HEALTHCHECK --interval=2s --timeout=3s --retries=10 CMD nc -z localhost 445` (the `slow` variant uses
   `--timeout=5s --retries=15`). The base image is `alpine:3.21` with `samba`, and `nc` (BusyBox) is present — the probe
   already works. So the health **signal** is there; what's missing is (a) a `restart:` policy to act on it and (b) any
   consumer of the health state.
3. **Nothing reads health today.** `start.sh` (and `e2e-linux.sh::start_smb_containers`) wait via an active TCP probe on
   the **host** port (`/dev/tcp/127.0.0.1/$host_port`), not `docker ... --wait` on container health. The orchestrator
   (`scripts/check/smb_orchestrator.go:61`) shells out to `start.sh` once and defers a single `Stop()`; no health
   gating. The post-flight probe (`e2e-linux.sh:520`) is diagnostic-only.
4. **Toolchain is modern.** Docker Engine 29.4.0, Compose v5.1.2. This Compose honors top-level `mem_limit` / `cpus`
   (the reliable non-swarm form) and merges multiple `-f` files.
5. **Playwright Linux lane.** `playwright.config.ts`: `timeout: 8000`, `retries: 0`, `workers: 1`,
   `fullyParallel: false`. CI shares this exact config (the Linux Docker lane sets `CI=true` and runs the same file), so
   load-induced `waitForSelector` / `skipParentEntry` timeouts hit CI too.
6. **Timeout-bound nuance — the global bump is not enough on its own.** No spec asserts on the 8 s value itself; raising
   it is safe for the global budget. **But** several helpers carry their own short inner budgets independent of the
   global timeout, and they throw first: `skipParentEntry`'s cursor-left poll (`helpers.ts:542`, `3000`), the
   focus-confirmation `waitForSelector` (`:413`, `3000`), the explorer-focus poll (`:445`, `3000`), and the
   cursor-landed-on-target poll (`:665`, `2000`). The twice-observed flake is the `skipParentEntry` throw, so the global
   bump alone leaves it unfixed. M2 raises all four explicitly — `:413` and `:445` to `6000` (they stack inside
   `ensureAppReady`, so they must sum under the `15000` global ceiling), `:542` and `:665` to `8000`. Longer existing
   budgets (`5000` / `10000` / `15000`) already have headroom and stay put.
7. **`retries: 1` collides with a written anti-pattern.** `docs/testing.md:139` has a `❌ retries: 1 to mask a race`
   entry: "Retries hide bugs… Drop retries when the cause is gone." The plan adopts retries **anyway**, scoped and
   justified (M2), and must update that doc rather than contradict it silently. Playwright marks retried-passes as
   `flaky` in its `list` reporter, so the signal stays visible, not silent.
8. **EACCES report verdict — out of scope, one-line-ish but not a clean one-liner.** The CI
   `EACCES /tmp/cmdr-e2e-report-linux.json` comes from `e2e-linux.sh:390`: the host pre-creates the report with `: >`,
   then bind-mounts it (`-v "$LINUX_E2E_JSON_REPORT:$LINUX_E2E_JSON_REPORT"`) into a container that writes it **as
   root**. The host file ends up root-owned; the next run's `: >` (run as the non-root CI user) fails `EACCES`. The fix
   isn't a single config line in the files this plan touches — it's a chown/`runuser` or a `--user` on the `docker run`
   in `e2e-linux.sh`, which risks breaking the in-container `pnpm install` / browser-cache assumptions. **Listed as out
   of scope**; track separately. It's cosmetic today (the report feeds only `scripts/e2e-test-timings/`, not the
   pass/fail gate), so it doesn't block this work.

## M1 — SMB fixture auto-recovery + resource limits (override file)

### Scope

Add `apps/desktop/test/smb-servers/.compose/docker-compose.override.yml` declaring, **per named service**, the keys
below. Enumerate the services explicitly; **exclude `smb-consumer-flaky`** (it cycles by design — see the loud rule).
The 14 services that get the override:
`smb-consumer-{guest,auth,both,50shares,unicode,longnames,deepnest,manyfiles,readonly,windows,synology,linux,slow,maxreadsize}`.
Caveat on `smb-consumer-slow`: its entrypoint applies a `tc qdisc` netem delay at start, which a restart resets — but
it's not in the E2E set (the e2e mode starts only guest/auth/50shares/unicode) and only the integration tests touch it,
so a restart there is harmless. Note-only; still give it the override for VM-pressure protection.

- `restart: unless-stopped` — a crashed `smbd` auto-recovers instead of staying dead for the rest of the run. The
  image's existing `HEALTHCHECK` already gives Docker a liveness signal to pair with it. **Restart is safe here because
  the fixtures are baked into the image** (the Dockerfiles seed `/shares/...` at build time; there are no volumes), so a
  mid-run auto-restart comes back with the same seeded data — there's no fixture state to lose. This is what makes
  restart-on-crash a clean win rather than a data hazard.
- `mem_limit: 256m` and `cpus: "0.5"` — generous for an Alpine + single `smbd` serving a handful of files (typical
  resident set is tens of MB). The cap exists to make `smbd` a **bounded** tenant so a build spike in the same VM can't
  let one container balloon and trigger the kernel to evict another — not to squeeze them. Use the top-level `mem_limit`
  / `cpus` form (honored by Compose v5 outside swarm), not `deploy.resources` (swarm-oriented; ignored by plain
  `compose up` on some setups). If `docker inspect` after `up` shows `Memory: 0`, the form was wrong — switch and
  re-verify.
- Optionally surface health to remove a fixed wait: if wiring `start.sh` to `docker compose up -d --wait` (gates on
  `healthy`) is a clean one-liner, take the free win — it replaces the bespoke TCP-probe loop with Docker's own health
  gate and means containers are confirmed-healthy, not just port-open. If it fights the `all`-mode service resolution or
  the 60 s deadline logic, **skip it** — out of scope. The TCP probe already works; this is a nicety, not a requirement.

Wire the override at **exactly one call site**: the `docker compose … up -d` in `start.sh:58`. That's the only
subcommand that applies `restart:` / `mem_limit` / `cpus`, so it's the only place the override `-f` is needed. Add
`-f "$COMPOSE_DIR/docker-compose.override.yml"` alongside the existing `-f "$COMPOSE_DIR/docker-compose.yml"` there.
**Do NOT touch the bare `docker compose -p smb-consumer` subcommands** — `ps` / `port` / `logs` / `down` at
`e2e-linux.sh:271,301,316,341,525` and the `ps` poll at `desktop-rust-integration-tests.go:110`. They reconstruct config
from container labels (the restart policy and limits are already baked into the running containers by `up`-time), so
they work unchanged; adding `-f` there is noise, not correctness. The orchestrator (`scripts/check/smb_orchestrator.go`)
shells out to `start.sh` / `stop.sh`, so it inherits the override for free — no edit needed.

### Intentions

- Resilience without drift: the override is cmdr-owned and re-vendor-safe; the vendored files stay pristine.
- Bounded tenants, not throttled ones: limits prevent eviction cascades, they don't slow the happy path.
- Respect the `flaky` contract: it's excluded by enumeration, not pattern.

### Test plan

- `./apps/desktop/test/smb-servers/start.sh e2e` (or `core`), then for each consumer:
  `docker inspect <container> --format '{{.HostConfig.RestartPolicy.Name}} {{.HostConfig.Memory}} {{.State.Health.Status}}'`
  — expect `unless-stopped`, `268435456`, `healthy` (and the `flaky` container shows `no` / `0` / no health).
- `docker stats --no-stream` while the e2e set is up — confirm each `smbd` sits well under the 256m cap (sanity that the
  cap is headroom, not a squeeze).
- If the `--wait` win was taken: confirm `start.sh` still exits 0 in `all` mode and the 60 s deadline path still fires
  on a deliberately-broken container.

### DONE

Override file committed; `docker inspect` shows the restart policy + memory limit + `healthy` on every non-`flaky`
consumer; `flaky` unchanged; both `start.sh` and the Linux E2E lane bring the stack up through the override with no
Compose orphan warnings.

## M2 — Playwright Linux-lane timeout + scoped retry

### Scope

In `apps/desktop/test/e2e-playwright/playwright.config.ts`:

- Raise `timeout: 8000` → `15000`. Update the inline comment (currently "Tight default: 8 s…") to state the new budget
  and why: absorbs load-induced UI-wait jitter on the shared Docker VM and on CI. Keep the "specs that need longer call
  `test.setTimeout` with a reason" guidance.
- Add `retries: 1`, **CI-scoped**: `retries: process.env.CI ? 1 : 0`. This is the right scope because (a) the flake is
  load-induced and concentrated on the CI / Docker lane, (b) local dev runs should still surface a flake immediately
  rather than paper over it, and (c) the Linux Docker lane sets `CI=true`, so it inherits the retry without a separate
  knob. Reject the alternatives: global `retries: 1` hides races from local dev (violates `docs/testing.md`);
  Docker-lane-only via a bespoke env var is more wiring for no extra signal since `CI` already partitions the two
  worlds.
- Leave the existing `const retries = 0` MTP-shard comment block intact in shape; just change the value expression. Do
  **not** touch `workers` or `fullyParallel`.

**Raise the load-fragile inner helper budgets in `helpers.ts` — this is part of M2, not optional.** The global `timeout`
bump never touches these: each is a hard-coded budget on a poll or a `waitForSelector` inside a helper, which throws its
own error long before the 15 s test timeout would fire. The twice-observed flake throws at `skipParentEntry`'s inner
`pollUntil(…, 3000)` (`helpers.ts:542`), so leaving it conditional leaves the actual flake unfixed. Bump all four:

- `helpers.ts:413` — `ensureAppReady` focus-confirmation
  `waitForSelector('.file-pane .file-entry.is-under-cursor', 3000)` → **`6000`**.
- `helpers.ts:445` — `ensureAppReady` explorer-focus-landed `pollUntil(…, 3000)` → **`6000`**.
- `helpers.ts:542` — `skipParentEntry` cursor-left-`..` poll, `3000` → `8000`.
- `helpers.ts:665` — `moveCursorToFile` cursor-landed-on-target `pollUntil(…, 2000)` → `8000`.

**`:413` and `:445` get `6000`, not `8000`, because they stack against the global ceiling.** Both live inside
`ensureAppReady` and run sequentially with no early return on the failure path (it `waitForSelector`s at `:413`, then
`pollUntil`s at `:445`), so their failure budgets sum: `8000 + 8000 = 16000` would exceed the new `15000` global
`timeout`, and Playwright would abort the test on the global timeout mid-`:445` — converting `ensureAppReady`'s precise
"focus did not land inside .dual-pane-explorer" diagnostic into a generic global-timeout abort. `6000 + 6000 = 12000`
stays under `15000`, preserving both the load headroom and the precise error. `:542` (`skipParentEntry`) and `:665`
(`moveCursorToFile`) keep `8000`: each is the **only** bumped budget inside its own helper, and neither helper is called
from within `ensureAppReady` — they're separate, test-invoked mid-flow — so no single synchronous failure path runs two
bumped budgets back-to-back. They don't stack (verified against the call graph).

**Intra-helper stacking vs the global ceiling — re-check on every future budget bump.** When raising any inner helper
budget, sum the bumped budgets that can run sequentially with no early return inside one synchronous failure path (the
clearest case: two awaits in the same helper, like `ensureAppReady`'s `:413` + `:445`) and keep that sum under the
global `timeout`. Otherwise a stacked overrun gets swallowed by the global-timeout abort and the helper's specific error
message is lost.

**Leave the longer budgets alone** (the `5000` / `10000` / `15000` waits and `test.setTimeout(15_000+)` sites) — those
already have load headroom. **Poll-vs-deadline rationale:** these are polls / readiness waits, not assertions of "must
finish within N ms," so raising the budget costs nothing on a green run — a passing test resolves the moment the
condition holds and never reaches the ceiling. The only thing the higher number changes is the failure budget: under
load, the helper waits longer before giving up instead of throwing a false "cursor did not leave" / "focus did not land"
error. (Pick another load-tolerant value if a specific helper measures slower, re-checking the stacking sum above.)

Reconcile the docs: `docs/testing.md:139` (`❌ retries: 1 to mask a race`) must gain a carve-out paragraph — retries are
allowed **CI-only, for load-induced environment flake on the shared-VM Docker lane**, and Playwright's `flaky` marker
keeps the signal visible so a retried-pass is still a tracked event, not a silenced one. The anti-pattern still stands
for masking a real race in app/IPC code; the carve-out is narrow.

Before declaring the timeout bump safe, confirm no spec relies on the 8 s **upper** bound (a test asserting "this must
fail within 8 s"). Grep already shows none assert on `8000`; re-confirm.

### Intentions

- Cheapest fix for the highest-frequency flake (`waitForSelector` / nav timeouts under load), with the signal kept
  visible.
- Honesty over silence: the doc carve-out makes the retry a documented, scoped exception, not a quiet contradiction of
  our own testing philosophy.

### Test plan

- `cd apps/desktop && pnpm vitest run` is irrelevant here; instead typecheck the config via the fast lane's
  `e2e-linux-typecheck` (don't run the full suite per the task constraint — this is the verification the implementer
  runs post-approval).
- One `pnpm check --include-slow` run including `desktop-e2e-linux` + `desktop-e2e-playwright`, on the historically
  flaky combination, green. A retried-pass shows as `flaky` in the `list` reporter — acceptable, and a useful
  confirmation the retry path is live.

### DONE

Config shows `timeout: 15000` and `retries: process.env.CI ? 1 : 0`; the four inner helper budgets
(`helpers.ts:413,445,542,665`) raised; `docs/testing.md` carve-out added; full suite + `desktop-e2e-linux` green; one
`--include-slow` run green.

## Acceptance

A flake fix can't be proven in one run. DONE for this plan is:

1. Config **verifiably applied**: `docker inspect` shows restart policy + memory limit + `healthy` on every non-`flaky`
   consumer (M1); the Playwright config shows the new timeout + CI-scoped retry (M2).
2. The standard gates green: full `pnpm check`, plus `desktop-e2e-linux`.
3. **One** `pnpm check --include-slow` run of the historically flaky combination, green.

**Real acceptance is a week of CI runs** with no SMB-eviction cascade and no load-induced `waitForSelector` flake — not
a single green run. State this to David at hand-off; the single green run proves the config is wired, not that the flake
is gone.

## Out of scope (tracked, not done here)

- **EACCES on `/tmp/cmdr-e2e-report-linux.json`** — root-owned bind-mounted report file (see finding 8). Needs a
  `--user` / chown change in `e2e-linux.sh` that risks the in-container install assumptions. Cosmetic today (feeds only
  the timing tool). Separate task.
- Lane serialization and the orchestrator health-gate-retry loop — the structural options, explicitly excluded.
- Capping the E2E app container tightly — rejected; OOM-kill risk on a `--privileged` Tauri + WebDriver + chromium
  container outweighs the eviction it would prevent. Leave uncapped, or set only a generous floor if a later measurement
  justifies it.
