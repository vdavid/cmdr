# i18n screenshots: couple a screenshot to every catalog key

Give every message-catalog key visual context by populating its `@key.screenshot` field, so a translator (agent or
human) sees the string IN the running UI, not just its description. This is the additive follow-on to the merged i18n
migration. Read `docs/guides/i18n.md` § Screenshots and `apps/desktop/src/lib/intl/messages/{CLAUDE,DETAILS}.md` first.

## Goal

After this: most catalog keys in `messages/en/<area>.json` carry `@key.screenshot: "<surface>.png"` pointing at an image
in `apps/desktop/src/lib/intl/messages/screenshots/` that shows that string in context. One screenshot serves many keys
(many keys naming the same file). The whole capture is a **re-runnable harness** (`pnpm i18n:shots` or similar), so when
the UI changes you re-run and the screenshots + couplings refresh — no manual key→file bookkeeping.

## Why this shape (intentions)

- **Auto-couple via runtime instrumentation, not manual mapping.** The hard part isn't taking screenshots, it's knowing
  WHICH keys appear on WHICH surface. Hand-maintaining that for ~2,050 keys across ~50 surfaces would rot instantly.
  Instead, instrument the runtime so it records the keys it actually resolves per surface; the screenshot of that surface
  then coantically couples to exactly those keys. This is Tolgee's in-context-capture idea, home-grown, committed to git,
  no TMS (consistent with the migration's no-TMS decision).
- **Drive with Playwright E2E, not the MCP.** The `test/e2e-playwright/` harness reliably launches + drives the real
  Tauri app and can screenshot. The tauri/cmdr MCP is flaky to connect from a fresh agent session (it needs the running
  instance's bridge port; it dropped repeatedly mid-effort). Build on the proven harness.
- **Spike-first.** Prove instrument→drive→capture→couple end-to-end on 2-3 surfaces before scaling, with a hard
  stop-and-report gate — same discipline that de-risked the runtime migration.
- **Optional for shipping a locale.** Description + key already let a translator work; screenshots are a quality booster.
  So this never blocks localization; it's purely additive.

## Non-goals

- **No new strings, no copy changes, no locale files.** Purely additive: the capture harness + screenshot PNGs +
  `@key.screenshot` metadata. `pnpm intl:keys` output is unchanged (screenshots are stripped metadata).
- **No production behavior change.** The capture instrumentation is dev/test-only, env-gated, and a zero-overhead no-op
  when off (it sits in the render hot path + the column-measurement fold, so this is non-negotiable — see Decision 1).
- **No language-specific content.** Screenshots are of the English UI; English-only catalog stays.
- **Not a visual-regression / pseudolocale-overflow tool.** That's a separate future effort; this only couples context
  screenshots. (Note the overlap for later: the same driver could later drive a pseudolocale for overflow testing.)

## Resume from the spike WIP (don't rebuild)

The prior spike's harness is already committed on this branch (`i18n/screenshots`) as `99f9fd07` ("WIP: i18n screenshot
capture harness (from spike, unverified)") and present in the worktree — no stash to pop. It contains:
`apps/desktop/scripts/i18n-capture.js`, `apps/desktop/scripts/couple-screenshots.js`,
`apps/desktop/test/e2e-playwright/i18n-capture.spec.ts`, plus the dev capture-mode instrumentation in
`messages.svelte.ts` / `messages.svelte.test.ts`, and `package.json` + `playwright.config.ts` wiring. It was mid-flight,
**never proven end-to-end**. **First execution step: read those files, validate/finish them against the design below**
rather than starting clean — then prove the loop (M0 gate).

## Design decisions

### Decision 1: capture-mode instrumentation (zero prod overhead)

Add to `$lib/intl/messages.svelte.ts` an env-gated capture mode (`CMDR_I18N_CAPTURE=1`, or a Vite-define so it
dead-code-eliminates in prod): a module-level `surface → Set<key>` sink, `setCaptureSurface(label)`, and a recording
hook in `t()` / `getMessage()` (and the `<Trans>` key path) that adds the resolved key to the current surface's set.
**Guard it so it's a no-op when off** — a single `if (CAPTURE)` at the top, ideally compiled out in prod builds; these
functions run per-visible-entry in render AND in `measure-column-widths.ts`'s fold, so any always-on cost regresses
scroll. Expose the sink for the driver to read (a `window.__i18nCapture` global in the webview, dumped to JSON at the
end of a run). Validate the guard with a quick perf sanity check (capture-off path identical to today).

### Decision 2: the Playwright capture driver

A capture spec/runner under `test/e2e-playwright/` (resuming the stash's `i18n-capture.spec.ts`) that, for each surface:
sets `CMDR_I18N_CAPTURE=1`, calls `setCaptureSurface('<label>')`, navigates/opens the surface, waits for render, reads +
clears the rendered-key set, and screenshots into `messages/screenshots/<surface>.png`. Reuse the existing E2E launch
lifecycle (`pnpm check desktop-e2e-playwright` mechanics; see `test/e2e-playwright/CLAUDE.md`). Screenshot the relevant
window (multi-window surfaces — viewer, settings, shortcuts — need the right window targeted). Output: the screenshots +
a `surface → keys` JSON manifest.

### Decision 3: the auto-coupler

A script (`scripts/couple-screenshots.js`, resume the stash) that reads the `surface → keys` manifest and writes
`@key.screenshot: "<surface>.png"` into the right `messages/en/<area>.json` for every captured key. Idempotent
(re-running is a no-op if unchanged; updates on UI change). A key seen on MULTIPLE surfaces: pick the most-specific (the
surface where it's the primary content) or the first; document the rule. Preserve catalog formatting (it's oxfmt'd JSON;
match the `''`-fix script's line-surgical approach or round-trip + oxfmt). NEVER touch message values, only the `@key`
`screenshot` field.

### Decision 4: coverage honesty (no silent gaps)

The coupler logs, per area: keys captured, keys NOT captured (never rendered on any driven surface), and why-likely
(dynamic-only key, hard-to-stage surface). Emit a coverage report artifact. Do NOT silently leave keys uncoupled and
imply full coverage — `docs/guides/i18n.md` says screenshots are optional, so partial coverage is fine, but it must be
VISIBLE which keys lack one.

## Surface inventory (drive all of these)

- **Windows**: main dual-pane explorer (incl. brief + full views, status bar, function-key bar); Settings + each
  section (Appearance, Behavior, AI cloud/local, File systems, Viewer, Developer, Updates, License, Advanced, Keyboard
  shortcuts); File viewer (toolbar, status bar, search, media, context menu); Keyboard shortcuts window; About window.
- **Dialogs**: copy/move transfer (picker + progress + error), delete/trash confirm, new-folder, new-file,
  conflict-resolution matrix, rename + extension-change, go-to-path, connect-to-server, error-report, crash-report,
  feedback, commercial-reminder + expiration.
- **Toasts**: transfer-complete, download teaching/summary, low-disk, AI suggestion, MTP connected, command-handler
  (favorites/tabs/zoom), latest-download.
- **Other**: empty states; the onboarding wizard steps (FDA, AI, beta, optional); the command palette; query-ui search
  + select dialogs + filter-chip popovers.

## Staging needs (surfaces automation can't reach naively)

Enumerate in the harness; for each, mock vs needs-David:
- **Paid-license states** (About/commercial-reminder/expiration): the `CMDR_MOCK_LICENSE=commercial` dev flag — automatable.
- **FDA-gated flows** (onboarding FDA step, downloads FDA hint): check the FDA gate mocks (`fda_gate.rs` /
  `is_fda_pending_runtime`); likely mockable via a dev flag — verify.
- **Error / crash states** (error-report, crash-report, friendly errors, write-error dialogs): trigger via the E2E
  fixtures / fault injection the suite already uses for conflict/error specs; mostly automatable.
- **Live MTP/SMB device states** (MTP connected, SMB reauth, network browser): SMB has the Docker fixture stack the E2E
  suite uses (automatable); MTP needs a real Android device → **needs David's hand**, or accept those few keys uncoupled
  (logged per Decision 4).

## Milestones

### M0 — Spike (gate)
Resume the stash, get the loop working end-to-end on 2-3 reliably-reachable surfaces (the main window, one Settings
section, one dialog e.g. delete-confirm): instrument records keys, driver screenshots, coupler writes `@key.screenshot`,
spot-check that a coupled key's screenshot actually shows that string. **Gate: if the loop can't be proven (capture
mode, driving, or coupling breaks), STOP and report exactly where.** Confirm the capture guard is a true prod no-op.

### M1 — Harness hardening
Finalize the instrumentation (guarded, tested off-path), the driver (surface-label API, window targeting, the
`surface → keys` manifest), the coupler (idempotent, formatting-preserving, multi-surface rule, coverage report), and a
single re-run entry point (`pnpm i18n:shots`). Docs: update `docs/guides/i18n.md` § Screenshots from "in progress / not
yet run" to the real mechanism + the re-run command, and `messages/CLAUDE.md`/`DETAILS.md` for the populated
`screenshot` field. A check or note so screenshots don't silently go stale (optional: a freshness warning).

### M2 — Scale across all surfaces
Drive the full surface inventory, staging the special cases (license/FDA/error via mocks; SMB via the fixture stack;
flag MTP as needing David). Couple all captured keys. Emit + commit the coverage report (which keys/areas are covered vs
not + why).

## Test strategy

- **TDD (real red→green)** for the pure logic: the coupler (given a manifest, writes the right `@key.screenshot`, is
  idempotent, never touches values) and the capture sink (records keys per surface; off = no-op). The driver + capture
  are integration (E2E), proven by the spike.
- **Don't break existing tests/checks**: `messages.svelte.test.ts` (capture mode added behind the guard), the parity
  tests (unchanged — values untouched), `pnpm intl:keys` (union unchanged), `desktop-message-key-naming` (the `@key`
  twin with a `screenshot` field still validates). Full `pnpm check -q` green at each milestone.

## Checks to run

`pnpm check -q` per milestone (bare, never piped — `no-tail-checker`). Specifically: `svelte-check`, `desktop-svelte-eslint`
(the capture code must lint clean + stay out of prod), `svelte-tests`, `message-keys-fresh`/`-naming`, `oxfmt` (catalog
JSON formatting after coupling). The E2E capture run is its own command, not part of the default lane.

## Parallelization

Mostly sequential (M0 gates the design; M1 the harness gates M2). Within M2, surfaces are independent captures but write
to the SHARED catalog files + the shared coverage report, so parallel capture runs race on those — serialize the coupling
step (capture in parallel if needed, couple once). Given no rush, sequential is fine.

## Gotchas (carried from the migration)

- Capture instrumentation MUST be a prod no-op (Decision 1) — it's in the hot path.
- The coupler must preserve oxfmt'd JSON formatting and touch ONLY the `@key.screenshot` field (mirror the
  apostrophe-fix script's discipline: never alter message values).
- Multi-window surfaces (viewer, settings, shortcuts, about) need the driver to target the right window.
- The error pipeline's keys are dynamically built (`getMessage(\`errors.listing.${reason}.title\`)`), so the capture
  hook in `getMessage()` records the RESOLVED key (good — that's exactly how to couple them, since the static scanner
  can't). Make sure the hook captures both `t()` and `getMessage()`.
- `<Trans>` resolves its key through the runtime too — ensure its key is captured.

## Definition of done

- `pnpm i18n:shots` (re-runnable) captures the surface inventory, screenshots in `messages/screenshots/`, and couples
  `@key.screenshot` across the catalogs; `pnpm check -q` green; values/union unchanged.
- A committed coverage report shows which keys/areas have a screenshot and which don't (+ why), with the MTP gap (if
  any) flagged for David.
- `docs/guides/i18n.md` § Screenshots reflects the shipped mechanism + the re-run command.
- Capture mode is a verified production no-op.

## Fresh-eyes review findings (folded in 2026-06-16 — these correct the spec above)

A reviewer read this spec against the committed spike (`99f9fd07`) and the real harness/E2E APIs. Key outcome: the
committed spike DIVERGES from the design as first written, and has a few guaranteed breaks. Treat the items below as
authoritative where they contradict the sections above.

Spec-vs-code drift (reconcile the spec TO the committed code, which is the more-evolved truth):
- **Capture gating (corrects Decision 1).** The code does NOT use `CMDR_I18N_CAPTURE=1` / a Vite `define` /
  `window.__i18nCapture`. It gates on `getAppMode() !== 'prod'` (runtime, from `import.meta.env.DEV` + the
  `CMDR_E2E_MODE` backend flag) and exposes `window.__cmdrI18nCapture`. Either adopt the code's approach in the spec, or
  consciously switch — but pick one.
- **"Zero prod overhead" is NOT met as built (corrects the non-goal + DoD).** The hook is `if (captureActive) record(key)`
  with `captureActive` a runtime `let` — a single always-false, well-predicted branch that DOES compile into the prod
  bundle (it inherits into `tString`, which `measure-column-widths.ts` calls in its fold). So "verified production no-op"
  is unsatisfiable as written. DECISION for execution (recommend (b)): (a) accept it and reword the promise to
  "negligible single-branch overhead, not zero"; or (b) gate behind a build-time-eliminable constant (`import.meta.env`
  / Vite `define`) so the dead-code-eliminator strips it, and verify by grepping the prod bundle. Resolve this — it's
  the central non-goal.
- **"stash" references are stale (corrects Decisions 2-3 + Milestones).** The WIP is the committed `99f9fd07`, not a
  stash; replace every remaining "the stash" / "resume the stash" with "the committed WIP". (Typo too: the intentions
  section has "coantically" → "semantically".)

Guaranteed M0 breaks in the committed spike — fix these FIRST:
- **Screenshot dir mismatch.** The capture spec writes to `src/lib/intl/messages/screenshots/` but `couple-screenshots.js`
  reads the report from `messages/en/screenshots/`. They never meet → `i18n:couple` always errors. The docs say
  `screenshots/` is a sibling of `en/`, so the SPEC is right and the COUPLER is wrong: fix the coupler's report/dir path
  to `messages/screenshots/` (catalog writes stay under `messages/en/`).
- **`.ts` import under plain node.** `scripts/i18n-capture.js` does `await import('../test/e2e-shared/fixtures.js')`
  (resolves `.ts`) but runs via `node` — plain node can't import TS. Run it under `tsx` (like the codegen scripts) or
  reach `createFixtures` another way.
- **Coupler may mint check-failing `@key` twins.** When a key lacks a twin, the coupler creates `{ screenshot: "x.png" }`
  with no `description`. Verify `desktop-message-key-naming`'s twin schema doesn't require a non-empty `description`; if
  it does, skip keys lacking a description rather than minting bare twins.

Coverage truths to bake into the design:
- **Snapshot-resolved strings need capture-on-before-trigger, not `rerender()`-after (extends Decision 1/2 + Decision 4).**
  The spike's `rerender()` bumps the locale rune to re-run reactive `t()` in MOUNTED markup. But many transient strings
  resolve as SNAPSHOTS in plain `.ts` at emit time — e.g. `settings-applier.ts:239` `addToast(tString(...))`, and the
  dynamic `getMessage(\`errors.listing.${reason}.title\`)` keys. For those, `rerender()` records nothing; capture must be
  ENABLED BEFORE the action fires (the `getMessage`/`t` hook records the resolved key only if capture is active at
  resolution time). The whole "Toasts" category + error keys are snapshot-resolved. Drive them by enabling capture, then
  triggering the action; accept that keys resolved before `enable()` are unrecoverable, and log them (Decision 4).
- **MTP is largely automatable — there's a `virtual-mtp` Cargo feature (corrects Staging + Open Question 2).** Existing
  MTP E2E specs run via `--features playwright-e2e,virtual-mtp` (see `apps/desktop/test/CLAUDE.md`, `mtp-fixtures.ts`).
  The connected toast, MTP volume, and browse surfaces are capturable without hardware. Re-scope: only genuinely
  device-specific nuances (real reauth, real device naming) might need David's device — verify at M2 before asking.
- **Some inventory surfaces are "stage-or-defer", not flatly drivable (refines the inventory):** crash-report dialog (the
  app must actually crash — can't screenshot a crashed webview), commercial-reminder/EXPIRATION (needs `CMDR_MOCK_LICENSE`
  + possibly an expired-state mock / time manipulation — verify the mock supports expired, not just `commercial`),
  auto-dismiss toasts (capture must land in the visible window — pin the toast or capture fast), download/latest-download
  toasts (depend on backend download events). Mark these explicitly stage-or-defer.

Lower-priority:
- The coupler round-trips whole catalogs through `JSON.stringify` + oxfmt (the allowed option, not line-surgical). Add a
  TDD test asserting "values byte-identical, only `@key.screenshot` changed" against a real catalog before scaling.
- `pnpm i18n:shots` (the single re-run entry in Goal/DoD) doesn't exist yet — `package.json` has `i18n:capture` +
  `i18n:couple`. M1 adds the chained `i18n:shots`.

What's solid (reviewer-confirmed): runtime-instrumentation-as-coupling-source is the right call (dynamic `errors.*` keys
can't be statically scanned); the `tauri-playwright` fork genuinely supports native per-window screenshots + multi-window
targeting (`window(label)`, `openSettingsWindowViaProd`); all spike helper refs resolve; the coupler preserves the
strip-on-load metadata model so parity + the key union stay untouched; the M0 stop-and-report gate is well-placed.

## Open questions for David

1. **Coverage bar**: aim for ~all reachable surfaces now (incl. SMB fixture states), or a high-value subset first
   (main + settings + the common dialogs/toasts) and the long tail later?
2. **MTP**: stage a real Android device for those few keys, or accept them uncoupled (logged)?
3. **Freshness enforcement**: want a check that warns when screenshots are stale relative to UI changes, or keep it a
   manual re-run?
