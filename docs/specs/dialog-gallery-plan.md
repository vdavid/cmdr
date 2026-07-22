# Dialog gallery: open every soft dialog on demand from Debug

## Why

Cmdr has 32 registered soft dialogs. Most are hard to evoke on purpose: the bulk-rename review needs an AI chat message
plus a wired-up agent, the transfer-progress dialog needs a live transfer, the stale-drive explainer needs an external
drive whose index actually went stale. That makes design review of the dialogs impractical, and they're the part of the
app most in need of one.

This adds a **Soft dialogs** section to the Debug window that lists every soft dialog and opens it on demand, in each of
its meaningful states, filled with fixture data.

**The core intent: a design-review instrument must not lie.** Everything below follows from that. Two consequences that
shape the whole design:

**1. The dialogs render in the main window**, not inside the Debug window. They're designed to sit on the two-pane
backdrop: the overlay deliberately starts at `inset: var(--titlebar-height)`, uses backdrop blur and macOS vibrancy, and
the reduce-transparency fallback keys off the main window's chrome. Also, `debug.json` is a deliberately minimal
capability and Tauri permissions **fail silently**, so dialog buttons that open windows or external URLs would look
broken for reasons unrelated to their design. And `ModalDialog` reports every mount to the Rust `SoftDialogTracker` via
`notifyDialogOpened`, which the MCP `dialog` tool and the E2E acks read: mounting a gallery copy in the Debug window
would tell the backend a dialog is open in the main window when it isn't. Rendering in the main window keeps that
honest, and MCP `dialog close <id>` then works on gallery-opened dialogs for free.

**2. Don't distort the dialogs to make them previewable.** A preview-only branch inside a dialog is a branch that can
rot, and it means you're no longer reviewing the shipping component. So: no `preview?: boolean` props, no dev-only
rendering paths inside dialog components. The gallery either passes real props, seeds the real state store, or emits the
real backend event. The only sanctioned component-side changes are the three listed under "Sanctioned component
changes" below, each justified on its own merits.

**Where the instrument must be honest about itself**: three dialogs don't live in the main window at all
(`delete-ai-model` is a settings-window dialog; `viewer-copy-confirm` / `viewer-copy-refuse` are viewer-window dialogs).
The gallery still shows them over the main window (hosting three extra windows isn't worth it), so **every gallery row
names the window the dialog actually lives in**. Reviewing a settings dialog over the file panes is fine as long as the
instrument says that's what you're looking at.

## Scope

- **30 dialogs opened on demand**, most in several states.
- **2 registered dialogs listed but not triggerable**: `search` and `transfer-progress` (David's call). Each shows an
  honest reason.
- **3 unregistered overlays listed for completeness** (command palette, network login form, pane volume chooser), noted
  as not part of `SOFT_DIALOG_REGISTRY` with how to evoke them by hand. The gallery claims to be a complete inventory of
  *registered soft dialogs*; these rows keep it from silently implying nothing else exists.

Out of scope: building the mechanisms `search` and `transfer-progress` would need. See "Follow-ups".

## Architecture

Read this whole section before starting any milestone.

### 1. `SOFT_DIALOG_REGISTRY` is the spine

`apps/desktop/src/lib/ui/dialog-registry.ts` already claims to be the single source of truth for soft dialog ids, and
`ModalDialog`'s `dialogId` is type-checked against it. The gallery enumerates from it, and a new
`dialog-gallery-coverage` check makes every registry id require a gallery entry. That's what stops the gallery from
rotting the moment someone adds dialog #33.

**Pre-existing drift to fix first (M1).** The type guarantee has two holes:

- `QueryDialog.svelte:283,335` casts `config.dialogType as SoftDialogId`, and `query-dialog-config.ts:182` types it
  `string`. As a result `selection-add` / `selection-remove` (`SelectionDialog.svelte:500`) are missing from the
  registry, so MCP's available-dialogs resource doesn't know they exist and `dialog close selection-add` reports an
  honest failure for a dialog that's genuinely open.
- `lib/tauri-commands/app-state.ts:66` types `notifyDialogOpened(dialogType: string)`. That's the real untyped seam:
  `OnboardingWizard.svelte:60` calls it with a bare literal and never touches `ModalDialog`.

Fix both, or the registry isn't actually the spine and the gallery inherits an incomplete list.

### 2. Transport: Debug window → main window (precedent already exists)

**This exact transport is already built and documented.** `routes/debug/DebugErrorPreviewPanel.svelte:154-168` emits
`emitTo('main', 'debug-trigger-transfer-error', { friendly })`; `routes/(main)/listener-setup.ts:405-419` consumes it
inside an existing `if (import.meta.env.DEV)` block; the seam is a recorded decision at `routes/(main)/DETAILS.md:165`.

So: follow it, don't reinvent it. The gallery's listener goes in **that same DEV block in `listener-setup.ts`**, and the
harness mounts from **`(main)/+layout.svelte`** (which already hosts `crash-report`, `error-report`, `feedback`,
`mtp-permission`, `ptpcamerad`), *not* `+page.svelte`. Reason: `+page.svelte` is 943 lines against a 854-line
file-length allowlist entry, so it's **already warning today**; `+layout.svelte` is 356 and `listener-setup.ts` is 482.
Don't add bulk to the file that's already over.

`+page.svelte` still needs a 2-line change (import the gallery's open-state store, add one clause to
`isModalDialogOpen()` at `:189`). **Without it, global shortcuts fire behind the previewed dialog**, which looks like a
dialog bug and would poison the review. Keep it to those 2 lines.

**Consequence to resolve, not ignore**: the existing `DebugErrorPreviewPanel` transfer-error trigger and the gallery's
`transfer-error` entry would be two ways to open one dialog from Debug, which is exactly the drift the gallery exists to
prevent. M2 folds it in: the gallery owns `transfer-error`, reusing the panel's `preview_friendly_error` IPC for
realistic fixtures, and the duplicate trigger is removed from `DebugErrorPreviewPanel` (its pane-error injection stays).

### 3. Three ways a dialog gets opened, decided by the dialog, not by preference

Which mechanism applies is a **property of how the dialog is already built**. Verify per dialog; don't assume from the
name:

- **Prop-driven**: the gallery renders the component with fixture props. Works only when the component takes everything
  it needs as props.
- **Store-seeded**: the component reads a module-level `$state` store and takes no content props (`BulkRenameReviewDialog`
  has no props at all and derives from `askCmdrState.renameReview`; `FeedbackDialog` and `ErrorReportDialog` self-gate
  on their flow stores and are mounted bare in `+layout.svelte`). Direct rendering isn't merely worse here, it renders
  **empty**. The gallery seeds the store and the app's own mount site renders it: the most faithful of the three, since
  the real trigger path runs.
- **Event-seeded**: the component self-mounts off a backend event (`StaleDriveDialog`). The gallery arranges the
  preconditions and emits the real event.

### 4. Dev-only, and provably absent from prod

Everything is gated on `import.meta.env.DEV`, which Vite inlines to a build-time boolean, so the gallery, its fixtures,
and every dialog import it adds tree-shake out of production (precedent: `settings/definitions/advanced.ts:29-33`). The
Rust fixture-dir command is `#[cfg(debug_assertions)]`.

**The fixture-dir IPC is called from the Debug window, not the main window.** `lib/dialog-gallery/` is not exempt from
`cmdr/no-raw-tauri-invoke` / `no-raw-bindings-import` (only `/routes/debug/`, `/lib/ipc/`, `/tauri-commands/`, and tests
are). `DebugDialogsPanel.svelte` is in an exempt path, so it resolves the fixture path and ferries it in the event
payload. This also keeps any reference to a `debug_assertions`-only command out of the main-window bundle.

M6 verifies the prod gating **empirically**, not by asserting tree-shaking works.

### Sanctioned component changes (the only ones)

1. `QueryDialog` / `query-dialog-config` / `app-state` registry typing (M1) — closes a real drift.
2. `delete-ai-model` extraction out of `AiLocalSection.svelte` (M4) — behavior-preserving.
3. A dev-only one-shot-flag reset in `indexing/drive-index-prefs.ts` (M5) — needed to make the stale dialog repeatable.

## File layout

```
apps/desktop/src/lib/dialog-gallery/          # new; NOT an i18n-enforced area, so fixture copy stays raw
  CLAUDE.md, DETAILS.md                       # colocated docs (C.md needs a D.md sibling; enforced)
  gallery-registry.ts                         # entries: id, label, hostWindow, status, reason, states
  gallery-state.svelte.ts                     # the open-state store `+page.svelte` reads
  DialogGallery.svelte                        # the main-window harness
  DialogGallery.a11y.test.ts                  # REQUIRED: a11y-coverage errors without it (see below)
  fixtures/*.ts                               # fixture data, grouped by area
apps/desktop/src/routes/debug/DebugDialogsPanel.svelte   # the Debug window list + triggers
scripts/check/checks/dialog-gallery-coverage.go          # + _test.go
```

Fixtures live under `lib/dialog-gallery/` deliberately: `routes/(main)/` is i18n-enforced, so fixture strings in a
`title` / `label` / `placeholder` / `aria-label` attribute there would trip `cmdr/no-raw-user-facing-string`. Passing
them as expressions from a non-enforced module keeps both the rule and the fixtures honest. **Never add fixture copy to
the i18n catalog.**

**`a11y-coverage` is error-level and scoped to `apps/desktop/src/lib`** (`desktop-svelte-a11y-coverage.go:30`): every
tracked `.svelte` there needs a colocated `*.a11y.test.ts` importing `$lib/test-a11y`. That applies to
`DialogGallery.svelte` (M1) and to the extracted `DeleteAiModelDialog.svelte` (M4) — note `AiLocalSection.svelte` is
allowlisted but a new sibling inherits nothing. Write the tests; don't touch the allowlist. Template:
`lib/ui/CLAUDE.md` § adding a tier-3 a11y test.

## Milestones

Sequential. Each milestone ends with `pnpm check -q` green **and** its colocated docs updated. Don't defer docs to M6.

**Pre-flight (before the first commit)**: add this plan to `docs/specs/index.md` under "In progress", or `docs-reachable`
(error-level) fails on the commit that tracks it.

### M1: spine, skeleton, and the coverage check

1. Registry drift: `QueryDialogConfig.dialogType: SoftDialogId`, drop both casts in `QueryDialog.svelte`, add
   `selection-add` + `selection-remove` to `SOFT_DIALOG_REGISTRY` with descriptions, and narrow
   `notifyDialogOpened` / `notifyDialogClosed` in `lib/tauri-commands/app-state.ts` to `SoftDialogId`.
   - Update `src-tauri/src/mcp/DETAILS.md:129` (it enumerates the soft-dialog close surface) in this milestone.
2. `gallery-registry.ts`: entry type + all 35 rows (32 registered + 3 unregistered overlays). Per entry: `id`, label,
   `hostWindow: 'main' | 'settings' | 'viewer'`, `status: 'ready' | 'not-triggerable' | 'unregistered'`, a `reason`
   (required unless `ready`), and named states. Only `alert` gets real states here; the rest are stubs for M2–M5.
3. `gallery-state.svelte.ts` + `DialogGallery.svelte`, mounted from `(main)/+layout.svelte` inside
   `{#if import.meta.env.DEV}`. Listener added to the existing DEV block in `listener-setup.ts`. Renders `alert` as the
   proof. Plus `DialogGallery.a11y.test.ts`.
4. The 2-line `isModalDialogOpen()` change in `+page.svelte:189`.
5. `DebugDialogsPanel.svelte` + a top-level `dialogs` section in `routes/debug/+page.svelte`'s `SECTIONS`, sibling to
   Components and Graphics. **Label it "Soft dialogs"**, not "Dialogs": `routes/debug/+page.svelte:92` already has a
   `components-dialogs` item labelled "Dialogs" (the `ModalDialog` primitive catalog), and two identically-labelled
   sidebar items is a bug in an instrument about UI quality. Each row shows its host window; non-`ready` rows render
   disabled with their reason visible.
6. `dialog-gallery-coverage.go` + `_test.go`: every `SOFT_DIALOG_REGISTRY` id has a gallery entry and vice versa.
   **Asserts id presence only, never state completeness**, or M1's own check red-flags M1's stub entries. Model on
   `desktop-svelte-ui-primitive-coverage.go`. Registration is four things, not one (`scripts/check/checks/CLAUDE.md:24,29,53`):
   the runner registration, an `Inputs:` declaration (else `TestEveryCheckDeclaresInputs` fails), a `.github/workflows/ci.yml`
   step (else `ci-coverage` fails), and the count table in `scripts/check/checks/DETAILS.md`.
7. **Docs pointers land now, not in M5**: `docs/architecture.md` must mention `apps/desktop/src/lib/dialog-gallery/`, or
   `docs-reachable` fails the moment M2 creates that dir's `CLAUDE.md`. Add the `docs/guides/building-ui.md` line here
   too (an agent building a dialog should learn the gallery exists and that adding an entry is expected).

**Tests**: `dialog-gallery-coverage_test.go` written **test-first, red→green** — it's the piece that has to fail
correctly for the guarantee to mean anything. Unit-test `gallery-registry.ts` for "non-`ready` entries carry a reason".

**Verify by hand**: `pnpm dev --worktree dialog-gallery` **from the worktree dir**, ⌘D, trigger `alert`, confirm it
appears over the **main** window and Escape closes it.

### M2: prop-driven fixtures, no disk needed

`about`, `commercial-reminder`, `expiration`, `extension-change`, `rename-conflict`, `mtp-permission`, `ptpcamerad`,
`archive-password`, `crash-report`, `transfer-error`, `viewer-copy-confirm`, `viewer-copy-refuse`, `license`,
`connect-to-server`, `selection-add`, `selection-remove`.

(`feedback` is **not** here: it's store-seeded, see M4.)

State coverage is the point, so give the ones with real variety real variety:

- `transfer-error`: one state per typed `WriteOperationError` variant (enumerate from the type). Reuse
  `preview_friendly_error` via the Debug panel, and delete the now-duplicate trigger from `DebugErrorPreviewPanel`.
- `archive-password`: first attempt and `wrongAttempt: true`.
- `expiration`: with and without `organizationName`.
- `rename-conflict`: newer-and-larger vs older-and-smaller (the dialog is a comparison, so one state reviews nothing).
- `ptpcamerad`: known blocking process and unknown.
- `selection-add` / `selection-remove`: dummy `FileEntry[]` with a realistic mix.

**Three of these expose only one state, and the gallery must say so rather than implying otherwise**:

- `license` (`LicenseKeyDialog.svelte:29`) and `about` (`AboutWindow.svelte:11`) take only `onClose`/`onSuccess`; every
  interesting state (existing-license panel, server-invalid retry, confirm-reset, loading) comes from
  `getCachedStatus()` and an `onMount` IPC. You'll see whatever the dev machine's license happens to be. Note it in the
  row. Stretch goal, only if it stays clean: seed the licensing store's cached status to unlock the other states.
- `connect-to-server` (`ConnectToServerDialog.svelte:27`) calls `triggerNetworkDiscovery()` in `onMount` **on purpose**,
  so opening it fires real mDNS and can raise the macOS Local Network prompt. Its `connecting`/error states are internal
  and unreachable via props. Say both in the row.

**Fixture data is part of the design review.** Include the cases that break layouts: a very long filename, a
deeply-nested path, a large file count with thousands separators, a multi-line error. A gallery of tidy 12-character
names hides exactly the problems this exists to surface. **Read
`apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts` first** — it already stages many of these dialogs for
screenshot capture, so the staging is largely solved.

**Docs**: write `lib/dialog-gallery/CLAUDE.md` + `DETAILS.md` this milestone. Per `AGENTS.md`, the how-to-add-an-entry
contract is `DETAILS.md` material; `CLAUDE.md` is must-knows only, under the 600-word ceiling.

### M3: fixture scratch directory and the disk-backed dialogs

`delete-confirmation`, `transfer-confirmation`, `mkdir-confirmation`, `new-file-confirmation`, `go-to-path` do real work
on mount: background scans, folder-suggestion streams, volume-space queries, path resolution. Faking that means faking
the very numbers the design displays, so point them at a **real throwaway directory** and let them behave for real.

1. Dev-only Rust command (`#[cfg(debug_assertions)]`) creating `<CMDR_DATA_DIR>/dialog-gallery-fixtures/` idempotently:
   a few dozen files across nested folders, varied sizes and names (include one very long name and one non-ASCII).
   Returns the path. Follow existing IPC conventions; `pnpm bindings:regen`. Called from `DebugDialogsPanel.svelte`
   (eslint-exempt path), path ferried in the event payload.
2. Wire the five dialogs. `delete-confirmation`: trash vs permanent, single vs many. `transfer-confirmation`: copy and
   move.
3. **`mkdir-confirmation` / `new-file-confirmation` need a live `listingId`, not just a path.** `NewFolderDialog.svelte:26`
   uses it at `:90` (conflict lookup), `:132` (directory-diff filter), and `:205` (`refreshListing`) — it's a pane-owned
   listing handle, not something a directory produces. So the gallery must navigate the focused pane to the fixture dir
   and pass that pane's real `listingId`. Skipping this makes the conflict check misbehave silently: the exact
   "renders broken, wastes the review" failure this plan is trying to avoid.

**Correct safety note for the docs** (the intuitive version is backwards): `DeleteDialog.svelte:56` and
`TransferDialog.svelte:65` take `onConfirm` as a **prop** and don't perform the operation, so a gallery no-op is
harmless wherever they point. `NewFolderDialog.svelte:192` calls `createDirectory()` **itself**, so mkdir and mkfile
genuinely write — those are the two the fixture dir actually protects.

**Test**: integration test that the fixture command is idempotent (running twice is safe and doesn't duplicate). It's a
data-writing path, so this one earns real coverage.

### M4: store-seeded dialogs, plus one extraction

1. **Store-seeded**: `bulk-rename-review`, `feedback`, `whats-new`, `error-report`, `operation-log`. Assign a fixture to
   the exported `$state` store and let the app's real mount site render it.
   - `bulk-rename-review`: all-allowed, some-rows-blocked, long-names (the one David called out).
   - `operation-log`: loading, populated, empty.
   - The gallery must **restore each store on close**, so a preview doesn't leave the app half-seeded. Part of the
     entry's contract, not an afterthought.
   - Document that `bulk-rename-review`'s Apply will fail on a fixture proposal (`ask-cmdr-trigger.svelte.ts` keys apply
     on a `proposalId` with backend-staged state, which a fixture has no counterpart for). Expected, not a bug.
2. **`onboarding` is NOT store-seeded** — the open flag is a local `let showOnboarding = $state(false)` in
   `+page.svelte:95`, rendered at `:841`; `onboarding-state.svelte.ts` exports `openWizard()` and step state, not an
   `open` field. Drive `+page.svelte`'s existing re-open path (`:297`, `:400`) instead. Note that
   `OnboardingWizard.svelte:185` is a hand-rolled `role="dialog"` overlay, not a `ModalDialog`. Add a per-step state if
   the state module exposes the step cleanly; if it doesn't, one entry is fine.
3. **`delete-ai-model` extraction**: currently an inline `<ModalDialog>` at `AiLocalSection.svelte:528`. Extract to
   `DeleteAiModelDialog.svelte` (props roughly `modelSizeFormatted`, `isDeleting`, `onConfirm`, `onCancel`), use it from
   `AiLocalSection`, add gallery states for idle and deleting, and write its `a11y.test.ts`. **Behavior-preserving**:
   same `dialogId`, same `role="alertdialog"`, same Enter handling, same disabled-while-deleting logic. Verify the
   settings window's real delete flow still works after.

### M5: event-seeded dialog and the honest gaps

1. **`drive-index-stale`**: self-mounts on the real `index-freshness-changed` event, gated on `indexing.staleNotify` and
   a persisted one-shot flag (`StaleDriveDialog.svelte:33-42`). The entry must arrange all three: ensure the setting is
   on, reset the one-shot flag (add a dev-only reset beside `hasShownFirstStaleDialog` / `markFirstStaleDialogShown` in
   `drive-index-prefs.ts`), then emit the real event. Going through the real event is the point; don't add an `open`
   prop.
   - Reset on **every** trigger: the dialog sets the flag when it shows, so it's repeatable only if you clear it each
     time.
   - **Use a volume id that's actually in the volume store.** `volumeName()` (`:24-28`) falls back to the raw id, so a
     synthetic id renders as `vol-abc123` in the body copy.
   - **Verify** the JS-emitted event actually reaches the JS listener (Tauri v2 round-trips it through the backend).
     Same "verify, don't assume" discipline as the `emitTo` transport.
2. **The gap rows, written for a reader who doesn't know the internals:**
   - `transfer-progress`: driven end to end by the backend's `write-progress` / `write-conflict` / `write-error` /
     `write-cancelled` / `write-settled` stream, so reaching its phases needs a scripted emitter. Mention that its scan
     phase and the conflict section are part of the same gap.
   - `search`: **deferred by decision, not blocked by an obstacle.** Don't invent a technical reason — `SearchDialog`
     takes four fixturable props, the index is live in dev, and it's the easiest dialog in the app to evoke by hand
     (⌘F, or the MCP `open_search_dialog` tool). The row should say it's reachable directly and simply isn't wired into
     the gallery yet. Publishing a false reason inside an instrument whose thesis is "must not lie" would be the worst
     possible detail to get wrong.
   - The three unregistered overlays (command palette, `NetworkLoginForm`, pane volume chooser): note they aren't in
     `SOFT_DIALOG_REGISTRY` and how to evoke them (⌘K for the palette).

### M6: verification and close-out

1. `pnpm check -q --include-slow`, green. Surface unrelated failures rather than fixing them silently.
2. **Prove the prod gating empirically**: `pnpm build`, then grep the built bundle for a named unique marker literal
   from `gallery-registry.ts` (pick one and state it, e.g. the section heading string) and confirm it's absent. An
   unnamed marker makes the check unfalsifiable.
3. Confirm `notifyDialogOpened` still reports correctly for gallery-opened dialogs (open one, check tracker / MCP
   available-dialogs), since honest MCP tracking is a stated reason for the main-window choice. Also confirm the
   Playwright E2E suite still passes, given M1 touches `isModalDialogOpen()` and the registry.
4. Strip milestone tags (`M1`, `M2`, …) from code, comments, and docs. The plan keeps them; the code doesn't.
5. Fresh-eyes agent runs the app via MCP, opens a sample across all three mechanisms plus one gap row, and reports
   anything broken.

## Constraints for every agent

- **Never modify a dialog component to make it previewable.** Only the three sanctioned changes above.
- Gate everything dev-only; prod must tree-shake it completely.
- Fixture copy stays out of the i18n catalog and out of i18n-enforced areas.
- Keep colocated docs in sync in the same milestone (`.claude/rules/docs.md`). `CLAUDE.md` = must-knows only, depth in
  `DETAILS.md`.
- **Don't hand-edit check allowlists** (`.claude/rules/file-length-allowlist.md`). If a length warning appears, surface
  it. Known pre-existing warn: `routes/(main)/+page.svelte` (943 lines vs 854 recorded) is already over before we touch
  it, which is why the harness mounts in `+layout.svelte`.
- `pnpm check` from the repo root, never raw `cargo` / `vitest`, and never truncate its output.
- Run the app from the worktree dir (`pnpm dev --worktree dialog-gallery`), or you'll QA main's frontend.

## Follow-ups (not in this effort)

- **A scripted backend emitter for the write-event stream** would make `transfer-progress` reviewable across every phase
  (including its scan phase and the conflict section) and would double as a way to exercise conflict and error paths
  that are painful to reach today. Highest-value follow-up by a distance.
- Wire `search` into the gallery with a canned result set.
- Host the settings- and viewer-window dialogs in their real windows instead of labelling the mismatch.
- `TransferConflictDialog` renders a bare `<div class="conflict-section">` designed to sit inside
  `TransferProgressDialog`'s body, not its own chrome. Rendering it standalone would need the gallery to fake its
  parent's chrome, which is the kind of lie this instrument shouldn't tell, so it's folded into the `transfer-progress`
  gap rather than getting its own row.
