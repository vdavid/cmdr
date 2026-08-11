# Plan: a Playwright-driven marketing screenshot pipeline

## Why this exists

The marketing masters in `brand/screenshots/` are shot by hand today: an agent drives a `pnpm dev` instance through the
Tauri MCP bridge, one call at a time, then a human-ish `screencapture` step. A full round takes 20–30 minutes of agent
round-trips, and the numbers it produces (the hero's pane rectangles) get measured in a separate ad-hoc step that can
drift out of sync with the shot.

Meanwhile the i18n translator capture (`pnpm i18n:capture`) does ~150 verified native screenshots in ~90 seconds,
because Playwright drives the real app over a Unix socket and every shot goes through one hardened `shoot()` helper.

**Goal**: marketing masters become one command that anyone (agent or David) can run after a UI change, producing the
same eight PNGs plus the hero geometry, with the failure modes caught by assertions rather than by eyeballing.

**Non-goal**: replacing the i18n capture, or making the marketing shots part of CI. This is a deliberate, run-it-on-
purpose tool that needs the machine left alone. It stays macOS-only.

## What the app has to look like (the constraints that drive every decision)

1. **A focused macOS window shadow.** Every master carries the focused-window shadow margins (112 left/right, 76
   top/bottom in device px). `app-main` lands on a 2508x1634 canvas around a 2284x1410 window at +112+76. The website
   hero's frame layer is mostly that shadow gradient, and `regenerate-hero.sh` derives each master's window rect from
   the PNG's own alpha bounding box, so a shot without the shadow silently produces a wrong hero.
2. **A prod-looking app.** No pink `DEV MODE`, no blue `E2E MODE`, no yellow `SCREENSHOT` title bar.
3. **Real content.** Real folders with real names and sizes, a real volume with real free space, a real git chip, real
   search results. Fixture trees look like a toy.
4. **A chat thread that reads like a real conversation** about the user's files.

## The three findings that shape the design

### The plugin's native screenshot has no shadow

`shoot()` photographs through the plugin's `native_screenshot`, which returns the **window rect only**. Measured on a
committed i18n master (`apps/desktop/src/lib/intl/messages/screenshots/acknowledgements.png`, 2026-08-11): 2160x1440,
alpha bbox `2160x1440+0+0`, corner pixel `srgba(0,0,0,0)` but the leftmost 100 columns average alpha 254.1 and columns
0–3 average 246.1. That is rounded-corner antialiasing, not a shadow ring.

**So the shutter stays `screencapture -x -t png -l <windowid>`**, which captures the window plus its real shadow on
transparency. Everything else (launch, staging, measuring, verifying) moves to Playwright.

Rejected alternative: synthesize the shadow in ImageMagick from the window's alpha mask. Deterministic and immune to
focus, but it is a fake macOS shadow, so the hero's look would shift and every future macOS shadow change would silently
diverge from what users actually see. `ideal-over-cheap`: keep the pixels honest.

### `CMDR_E2E_MODE` is what makes the app un-photographable, and it is optional

The Playwright plugin is gated on the **cargo feature** `playwright-e2e` alone (`src-tauri/src/lib.rs:285-292`),
independent of `CMDR_E2E_MODE`. And `CMDR_E2E_MODE=1` is exactly what:

- paints the blue `E2E MODE` title bar (`test_mode::is_e2e_mode`),
- sets `ActivationPolicy::Prohibited` so the app can **never become active**, and orders every window to the back
  (`e2e-playwright/DETAILS.md` § Multi-window testing).

A never-active app never becomes the key window, and macOS draws the wide shadow only for the key window. So an E2E-mode
run could not produce a marketing master even in principle.

**So the shots run launches the E2E binary with `CMDR_E2E_MODE` unset**: prod title bar, normal activation policy, a
window that can be made key. `guard_e2e_requires_data_dir()` only fires under E2E mode, and we pass `CMDR_DATA_DIR`
anyway, so nothing is lost.

Focus is then claimed from **outside** the app, per shot:

```
osascript -e 'tell application "System Events" to set frontmost of (first process whose unix id is <pid>) to true'
```

This is the same mechanism that worked in the manual MCP rounds, and it sidesteps the harness's documented inability to
take the front position from inside (`set_focus` on a non-LaunchServices binary is a no-op against an app that holds the
front).

### Real data comes from the data dir, and `CMDR_E2E_START_PATH` must stay UNSET

❗ **This is the data-safety crux of the whole plan.** The auto fixture in
`test/e2e-playwright/fixtures.ts:54-64, 122-133` diffs the start path against the fixture manifest after every test and
calls `restoreFixtureTree()`, which **deletes every entry not in the manifest**
(`test/e2e-shared/fixture-manifest.ts:227-233`). Point that at a real directory and the run erases it. The guard is
skipped only when `CMDR_E2E_START_PATH` is undefined in the _Playwright_ process — and `global-setup.ts:16-30` **creates
a fixture tree and sets the variable when it is unset**, so "just don't set it" is not enough on its own.

So the marketing shard must, explicitly and with a comment saying why:

1. leave `CMDR_E2E_START_PATH` unset in both the app and the Playwright process,
2. skip `global-setup`'s fixture creation for this shard kind, and
3. skip the post-test fixture guard for this shard kind.

Cmdr's first principle is protecting the user's data, and this run is the one place in the repo where test
infrastructure points at a person's real files. Treat any shortcut here as a blocker, not a nit.

With `CMDR_E2E_START_PATH` unset the app restores its panes from its own persisted state, which is what we want anyway:
the shots data dir remembers where the panes were. Navigation during a run goes through the MCP helpers
(`test/e2e-shared/mcp-client.ts`), exactly as the i18n surfaces already do.

Data dir: `~/Library/Application Support/com.veszelovszki.cmdr-shots`, passed as `CMDR_DATA_DIR`. Its own path, so it
can never collide with prod, plain dev, a worktree dev instance, or an E2E shard, and the instance lock
(`instance_lock.rs:180-191`) keeps two runs off it. It is **not** in the repo and **not** throwaway: keeping it warm is
what turns the ~20-minute whole-drive index reconcile into a one-time cost.

## Architecture

```
scripts/marketing-shots.ts  (orchestrator, Node)
   |  refuses to run if another Cmdr is up; ensures the data dir; seeds it on first run
   |  launches target/<host>/release/Cmdr   (playwright-e2e feature, CMDR_E2E_MODE unset)
   |  exports CMDR_SHOTS_PID + CMDR_PLAYWRIGHT_SOCKET
   v
playwright --project=tauri --shard-kind=marketing-shots  test/e2e-playwright/marketing-shots.spec.ts
   |  stages each shot through the real UI (settings, panes, tabs, search, rail, theme)
   |  measures the hero pane rects off the live DOM
   v
shootWithShadow()  in test/e2e-playwright/marketing-shots-helpers.ts
   |  osascript: bring the pid frontmost
   |  JXA: resolve the CGWindowID for that pid + window label
   |  screencapture -x -t png -l <id>
   |  verify: complete PNG, non-blank pixels, alpha bbox == expected window rect at the focused margins
   v
brand/screenshots/*.png  +  brand/screenshots/hero-cutouts.json
```

Everything under `test/e2e-playwright/` so the harness, helpers, and `shoot()`'s hard-won lessons are reused rather than
reimplemented; the orchestrator lives beside `i18n-capture.ts` for the same reason.

## Milestones

Each milestone ends green and committed. Run the narrow check the change touches (`check-scope-matches-change`), not the
full suite; `pnpm check -q` per milestone, `--include-slow` only at the end.

### M1 — `shootWithShadow()`, proven on a live window

**Intent**: prove the riskiest link (native shutter + focus + verification) before building anything around it. If this
does not work, the whole plan changes shape, so it goes first and alone.

Build `apps/desktop/test/e2e-playwright/marketing-shots-helpers.ts`:

- `frontmostByPid(pid)` — the `osascript` above; throws with an actionable message when Accessibility permission is
  missing.
- `windowIdFor(pid, title?)` — JXA over `CGWindowListCopyWindowInfo`, filtered to `kCGWindowOwnerPID === pid`,
  `kCGWindowLayer === 0`, largest area (or matched by title for the settings window). Throws when zero or ambiguous.
  Neither the CGWindowID nor the app's pid crosses the socket: the plugin resolves the id internally
  (`tauri-plugin-playwright/src/native_capture.rs:128-196,298-330`) and returns neither. So the orchestrator passes its
  own `appProc.pid` to the spec through the environment, and the spec resolves ids itself.
- `shootWithShadow(page, windowLabel, outPath, expected)` — mirrors `shoot()`'s contract and reuses its parts:
  `settlePaint`, `clearStrayToasts`, the IEND-complete wait (`isCompletePng`), `assessImageContent` from
  `i18n-capture-png.ts`, three attempts, and a `BlankShotError`-shaped failure. Adds one gate `shoot()` cannot have:
  **the alpha bounding box must equal the expected window rect at the focused margins** (112/76). An unfocused window
  yields 68/52 and fails loudly instead of shipping a hero with half the shadow.
- Export the margin constants (`FOCUSED_SHADOW_X = 112`, `FOCUSED_SHADOW_Y = 76`) with a comment naming what they are
  and how to re-derive them.

**Tests**: `marketing-shots-helpers.test.ts` (Vitest, in `apps/desktop/test/`), test-first, real red→green, over the
pure parts only: the alpha-bbox verdict (accept exact, reject 68/52, reject off-centre, reject a bbox that touches the
canvas edge), and the JXA output parser (zero windows, several windows, malformed JSON). The impure parts (`osascript`,
`screencapture`) are proven by M2's real run, not mocked.

**Manual proof for this milestone**: launch any window, call `shootWithShadow` against it from a scratch script, confirm
a 2508x1634 PNG with alpha bbox `2284x1410+112+76`.

**Checks**: `pnpm check -q desktop-vitest`, `pnpm check -q lint`.

### M2 — the orchestrator and the shots data dir

**Intent**: one command that gets a prod-looking, real-data app up and connected, and tears it down cleanly. Splitting
this from M3 keeps "can we drive it at all" separate from "does it look right".

`apps/desktop/scripts/marketing-shots.ts`, modeled on `i18n-capture.ts`:

- Refuse to run when another Cmdr is up (same check `i18n-capture.ts` uses), and **kill only its own pid** on exit. ❌
  Never `pkill -f 'target.*Cmdr'`: parallel worktree sessions are normal here, and that pattern killed the manual
  round's app twice mid-shoot.
- `--build` builds via the existing `pnpm test:e2e:playwright:build` (feature set unchanged, so the cargo cache is
  reused). Without it, fail with a clear message when the binary is missing or older than `src-tauri/`.
- Launch env: `CMDR_DATA_DIR=<shots dir>`, `CMDR_PLAYWRIGHT_SOCKET=<unique>`, `CMDR_MCP_ENABLED=1` +
  `CMDR_MCP_PORT=<ephemeral>` (the navigation helpers go over MCP), `CMDR_E2E_ASK_CMDR_FAKE=1`,
  `CMDR_SECRET_STORE=file`, and **unset**: `CMDR_E2E_START_PATH`, `CMDR_E2E_MODE`, `CMDR_I18N_CAPTURE_BUILD`.
  - `CMDR_SECRET_STORE=file` because without E2E mode the app talks to the **real macOS Keychain**
    (`secrets/mod.rs:112-124`), which can raise a blocking approval dialog mid-run. Nothing here needs a real key: the
    chat is seeded (M4).
  - ❌ Don't add `CMDR_MOCK_FDA` / `CMDR_MOCK_LICENSE`. They're debug-assertion-gated, so they'd need
    `--config profile.release.debug-assertions=true` on the build (what `i18n-capture.ts:372-387` does), which is a
    different cargo config and therefore a full rebuild instead of reusing the `pnpm test:e2e:playwright:build` cache.
    Avoid the need instead: shoot panes at paths that aren't TCC-protected (the repo, not `~/Documents`).
- **Things E2E mode suppresses that a prod-mode run will NOT.** `isE2eRun()` is false here, so the "What's new" popup
  and the upgrade-nudge toast are live (`src/routes/(main)/startup-gates.ts:117,138`). Both would land in a shot. Mark
  them seen in the seeded data dir (M4) and assert their absence before the shutter. Conversely, two behaviors we WANT
  come back: the rail really grows the window (`rail-window.ts:56,87`), and the settings window really raises
  (`settings-window.ts:68`).
- **No update check and no analytics from a shots run**, and both are ON by default in a prod-mode release build:
  - Analytics consent is granted unless the setting is explicitly false (`src-tauri/src/analytics/mod.rs:80-81,105`),
    and the prod PostHog project is chosen for a release build (`:89`). A shots run would otherwise register a fresh
    install id and emit real events. The seed sets `analytics.enabled: false`.
  - The updater runs a background poll loop driven by `updates.autoCheck` (`src/lib/updates/updater.svelte.ts:232-244`).
    The seed sets it false, which also kills the "Restart to update" toast that would otherwise photobomb a shot. Both
    are seeded settings, ❌ never code patches: a screenshot run must not be a special case inside the app.
- On first run, seed the data dir (§ "Seeding the shots data dir") and print the index-warm-up warning; on later runs,
  wait for `indexStatus: fresh` before handing over to the spec, with a visible countdown so a cold dir is obvious.
- Pass `CMDR_SHOTS_PID` and the resolved window id to the spec through the environment.
- Register `marketing-shots` as its own shard kind in `playwright.config.ts` (there is only one project, `tauri`;
  "shards" are `CMDR_E2E_SHARD_KIND` driving `testMatch` / `testIgnore` at `:8-27`). Four edits: a match regex beside
  `:15-20`, a branch in the `testMatch` ternary `:21`, an entry in `testIgnore` so `all` / `non-mtp` / `mtp` exclude it
  `:22-27`, and running with `CMDR_E2E_SHARD_KIND=marketing-shots` and **no** `--project` beside a positional path. ❌
  Do NOT add a `shardSpec` to `scripts/check/checks/desktop-svelte-e2e-playwright.go`: this must never run in CI or on
  Linux. The spec also asserts `process.platform === 'darwin'` and skips otherwise.
- The config's per-test timeout is 15 s (`playwright.config.ts:68`), far too short for a staged marketing shot. The spec
  sets its own generous `test.setTimeout`, like the i18n pass does, and the comment says why: on timeout Playwright
  destroys the plugin socket and every later shot fails with `Not connected`, which reads like a crash and hides the
  real message.

**Tests**: the shard-kind wiring is proven by the run itself. Add a Vitest for the "is this binary stale" comparison if
it grows past a one-liner; otherwise skip.

**Checks**: `pnpm check -q desktop` (typescript + lint), and one real launch that reaches
`waitForSelector('.file-pane')` and exits 0.

### M3 — the spec: staging, the eight shots, and the hero geometry

**Intent**: put every staging decision in one readable file, so a future UI change is a diff in a spec rather than an
evening of MCP calls.

`apps/desktop/test/e2e-playwright/marketing-shots.spec.ts`, one `test()` per master pair, each staging through the real
UI (settings commands, navigation, tabs, search dialog, rail toggle, theme toggle):

- `app-main-{dark,light}` — two panes, the pinned-tab arrangement, the F-key bar visible.
- `search-{dark,light}` — the search dialog over the panes with real results.
- `chat-{dark,light}` — the Ask Cmdr rail open on the seeded thread (§ below). The rail widens the window, so this pair
  lands on 3188x1634.
- `settings-{dark,light}` — the settings window (opened through `openSettingsWindowViaProd`, never by routing the main
  window), 1928x1426 around a 1704x1202 window.

Window sizing has no generic helper: `setWindowSize` is module-private in `i18n-capture-frame.ts:190-197`, and the
exported path (`fitSurfaceWindow`, `stressLayoutIfWorstCase`) is about fitting content, not hitting an exact canvas.
Lift a small `setWindowSize(page, label, w, h)` into the shared helpers rather than calling `plugin:window|set_size`
from the spec, so the two capture runs keep one way of doing it.

Staging rules learned the hard way, each of which becomes an assertion or an explicit step rather than a comment:

- **Wait for `indexStatus: fresh` before any pane shot.** A reconciling index puts an hourglass in every size cell and a
  `≥` on every folder size. The orchestrator gates on this, and the spec re-asserts it.
- **Close the rail before resizing for the pane shots.** With the rail open, each pane measures ~430 px instead of ~570
  px, which would poison the hero cutouts.
- **Set the cosmetics explicitly**, never inherit them: `appearance.appColor: cmdr-gold`,
  `appearance.fileSizeFormat: binary`, `listing.sizeUnit` (the setting that actually switches raw bytes vs dynamic),
  `appearance.showFunctionKeyBar: true`, `mediaIndex.enabled: false` (otherwise a "Text in images" band of personal
  thumbnails can appear).
- **Rebuild the tab arrangement from scratch**, and unpin before closing: `close` / `close_others` skip pinned tabs, so
  a seeded pinned tab survives a "clean up" and you end with three tabs in a pane.
- **The git chip** reads `main · dirty` in a dirty worktree. The spec points the panes at a path whose repo state it can
  assert, and fails rather than silently shooting a dirty chip.

Then the hero measurement, in the same spec, immediately after the `app-main` shots and from the same live DOM:
`.full-list-container` for each pane's left edge and width, `.listbox-region` for the top (below the column headers),
the container's bottom for the end (above the status bar), each rectangle inset 2 device px so the window border and the
pane divider stay in the frame layer. Written to `brand/screenshots/hero-cutouts.json`.

**Intent behind folding the measurement into the spec**: measure-and-shoot can no longer diverge. The drift that made
the hero cutouts wrong was exactly a hand-measured constant outliving the layout it was measured on.

**Tests**: the shots are the test. The pure geometry helper (rect → inset rect, and the "does this JSON match this
master" guard) gets a Vitest, written test-first.

**Checks**: `pnpm check -q desktop`, plus a real run of the spec, plus `apps/website/scripts/regenerate-hero.sh` when
that script is present (it lands with the `david-alternativeto-refresh` branch, see § Coordination).

### M4 — seeding the shots data dir

**Intent**: make the run reproducible from nothing, including the chat thread, without an API key and without a network
call.

`apps/desktop/scripts/marketing-shots-seed.ts` (invoked by the orchestrator on first run, and by `--reseed`):

- Settings and `app-status.json` defaults: onboarding complete, **"What's new" marked seen** (prod mode does not
  suppress it), rail width, favorites, the cosmetics above.
- Ask Cmdr consent, in `main.db`'s `meta` table (not settings): `ask_cmdr_consent_version = '2'` (the current
  `CONSENT_COPY_VERSION`) and `ask_cmdr_consent_at`. Both must be present or the rail renders the consent screen.
- One `conversations` row (highest `updated_at`, `archived = 0`, so `bootstrapActiveThread` picks it), plus `messages`
  rows with explicit `seq` and `content_blocks` JSON: a user question about the user's files, one or two real tool calls
  (`list_dir`, `important_folders` — names from the `AgentPart` wire enum), their results, and the assistant's answer.
  The FTS triggers keep themselves in sync.
- Optionally a `cost_meter` row plus `conversations.last_prompt_tokens` / `last_prompt_budget`, so the footer and the
  context gauge show real numbers instead of "not measured".

**Honesty note, and a David decision**: the thread's text is written by a model (me), not produced by a live provider
call. That is what David asked for, and it is honest in the sense that a model wrote it. The constraint that matters:
**the question, the tool calls, and the answer must describe what Cmdr actually does**. A marketing shot showing a
capability that does not exist is the one failure mode here that no assertion catches, so the seeded thread's copy is a
draft for David's review like every other human-facing string.

**Tests**: Vitest over the seed's SQL builder (a conversation with N messages produces N rows with contiguous `seq`
starting at 0, valid `content_blocks` JSON, and `role` values from the allowed set), written test-first. The seed is
then proven end-to-end by M3's `chat-*` shots.

**Checks**: `pnpm check -q desktop-vitest`, plus the real chat shots.

### M5 — docs, and retiring the manual path

**Intent**: leave exactly one documented way to do this, and make the guardrails findable at the moment they bite.

- Rewrite `docs/guides/screenshots.md` around the one command. Keep the parts that are still true and still non-obvious:
  the focused-shadow rule and why it matters to the hero, the "leave the machine alone" warning, the index-freshness
  gotcha, the pinned-tab trap, the rail-steals-pane-width trap, and the consumer list (README, hero, AlternativeTo,
  MacUpdate). Drop every MCP step and the CleanShot mention.
- `apps/desktop/test/e2e-playwright/CLAUDE.md`: one bullet for the new shard and the ❗ front-position warning, pointing
  at `DETAILS.md`. `DETAILS.md`: the shard's files, why `CMDR_E2E_MODE` is deliberately off, and the alpha-bbox gate.
- `brand/CLAUDE.md`: the screenshots bullet points at the command, not at a procedure.
- `docs/architecture.md`: a map line only, no mechanism.
- Check the `claude-md-length` and `docs-reachable` results; ❌ never add or raise an allowlist entry without David's
  explicit consent, surface a warn instead.

**Checks**: `pnpm check -q docs`, then a full `pnpm check -q` once.

## Seeding the shots data dir (reference)

- `main.db` lives beside `operation-log.db` in the data dir; schema version 3, stamped in `meta.schema_version`. Seed
  against an already-migrated file (launch the app once, quit, then seed) rather than hand-creating tables.
- `content_blocks` is a JSON array of externally-tagged `AgentPart`: `[{"text":"…"}]`,
  `[{"tool_call":{"call_id":"c1","tool":"list_dir","arguments":{…}}}]`,
  `[{"tool_result":{"call_id":"c1","content":{…},"elided":false}}]`. Omit `reasoning`; it is dropped before the UI.
- A tool result renders as failed only when `content.available === false` or `content.problem` is present.
- `askCmdrRailOpen: true` in `app-status.json` opens the rail at launch; the spec toggles it anyway, so this is
  convenience, not load-bearing.

## Risks, and what each one costs

- **Screen-recording permission.** `screencapture` needs it for the process that runs it. Verified working from this
  session's shell on 2026-08-11 (a 4x4 probe returned real pixels, mean 0.60). If a future runner lacks it, the shutter
  returns black and the blank-pixel gate fails loudly, which is the right failure.
- **Accessibility permission** for the `osascript` focus step, same shape of failure. Both are named in the helper's
  error messages so nobody hunts a Cmdr bug that is not there.
- **Something else holds the front position.** Still fatal, still the documented warning, now caught by the alpha-bbox
  gate as well as the blank-pixel gate.
- **A cold shots data dir** costs one whole-drive index reconcile (~20 minutes). Only once, and the orchestrator says so
  up front instead of producing hourglass screenshots.
- **The E2E binary is not a release build in every respect.** It carries `playwright-e2e` and the baked dialog gallery.
  With `CMDR_E2E_MODE` unset the app resolves to `prod` app mode, so the title bar is plain, but M1's first real shot is
  the moment to compare it against a real release build and record any remaining visible delta.
- **`marketing-shots.spec.ts` must never reach a Linux lane or CI.** Enforced by the shard exclusion; worth an explicit
  assertion in the spec that `process.platform === 'darwin'`.

## Coordination with the `david-alternativeto-refresh` branch

That branch (which David continues separately) already carries `brand/screenshots/hero-cutouts.json`,
`apps/website/scripts/regenerate-hero.sh`, the rewritten `apps/website/public/hero/DETAILS.md`, the updated
`brand/CLAUDE.md`, and freshly shot masters. This plan is branched off `main`, so none of that is here.

Rules for this branch, to keep the merge cheap:

- **Do not reimplement `regenerate-hero.sh` or the hero compositing.** This pipeline produces its _inputs_
  (`app-main-*.png` and `hero-cutouts.json`, in the exact schema that branch's script reads). Compositing stays there.
- **Do not commit new `brand/screenshots/*.png`.** Prove the pipeline by writing to a scratch out-dir (`--out <dir>`,
  defaulting to `brand/screenshots`), show David the results, and let him decide which set to keep. His branch's
  binaries stay authoritative until he says otherwise.
- `docs/guides/screenshots.md` will conflict: both branches rewrite it. The version here supersedes, because the manual
  procedure is what this replaces. Resolve in favour of this branch's file and re-check that every guardrail the other
  branch documented survives the merge (focused shadow, index freshness, pinned tabs, rail width, parallel-session
  `pkill`).

## What can run in parallel

Very little, and that is fine. M1 gates everything (if the shutter cannot produce a focused-shadow PNG, the design
changes). M4's seed script is independent of M2/M3 and can be written alongside them by a second agent, since it touches
only `marketing-shots-seed.ts` and its Vitest. M5 is strictly last.
