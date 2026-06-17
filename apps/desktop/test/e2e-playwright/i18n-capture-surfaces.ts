/**
 * Per-group surface-capture functions for the i18n screenshot-capture driver
 * (`i18n-capture.spec.ts`).
 *
 * One exported function per surface group (settings window, main-window overlays,
 * toasts, empty pane, onboarding, what's-new, indexing). Each stages the app to
 * its surfaces and records keys via the shared engines in
 * `i18n-capture-helpers.ts`. The spec sequences these in coupling order; this
 * module holds no orchestration of its own.
 *
 * Coupling policy (set by the spec's call order, mirrored here for context): a
 * key may render on several surfaces; the coupler assigns each key the FIRST
 * surface it appeared on, so order surfaces narrow-to-broad in the spec.
 */

import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { expect } from './fixtures.js'
import {
  ensureAppReady,
  dismissOverlay,
  skipParentEntry,
  moveCursorToFile,
  openSettingsWindowViaProd,
  closeScopedWindow,
  dispatchMenuCommand,
  getFixtureRoot,
  LOCAL_VOLUME_NAME,
  TRANSFER_DIALOG,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpSelectVolume, mcpNavToPath, mcpAwaitPath } from '../e2e-shared/mcp-client.js'
import { writeFile, waitForConflictPolicy, clickTransferStart } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import {
  type SurfaceEntry,
  captureCall,
  captureSurface,
  captureToastSurface,
  focusWindow,
} from './i18n-capture-helpers.js'

/**
 * Every Settings section to capture, in capture (coupling) order. `path` is the
 * English section identity the production `navigate-to-section` deep-link takes
 * (NOT the localized sidebar title), so passing the full SUBSECTION path lands on
 * real content rather than a parent's summary-card grid. `sectionId` is the
 * stable `data-section-id` on the rendered `<section>` (see
 * `SettingsContent.svelte`) used as the per-section readiness signal; `label` is
 * the capture surface name. Mirrors the section table in `accessibility.spec.ts`
 * and `SettingsContent.svelte` — keep in sync if a section is added, renamed, or
 * re-homed.
 *
 * EVERY section (including the first, Appearance › Colors and formats) is reached
 * by an explicit deep-link, never by relying on the window's default-rendered
 * section: that default is the last-viewed section restored from the persisted
 * store, which is non-deterministic (a prior session can leave it on "Advanced",
 * and a top-level section like "Appearance" renders a summary grid with no
 * `data-section-id` at all). Deep-linking each one makes the run deterministic.
 */
const SETTINGS_SECTIONS: { path: string[]; sectionId: string; label: string }[] = [
  {
    path: ['Appearance', 'Colors and formats'],
    sectionId: 'appearance-colors-and-formats',
    label: 'settings-appearance',
  },
  {
    path: ['Appearance', 'Zoom and density'],
    sectionId: 'appearance-zoom-and-density',
    label: 'settings-appearance-zoom',
  },
  {
    path: ['Appearance', 'File and folder sizes'],
    sectionId: 'appearance-file-and-folder-sizes',
    label: 'settings-appearance-sizes',
  },
  { path: ['Appearance', 'Listing'], sectionId: 'appearance-listing', label: 'settings-appearance-listing' },
  {
    path: ['Behavior', 'File operations'],
    sectionId: 'behavior-file-operations',
    label: 'settings-behavior-file-operations',
  },
  {
    path: ['Behavior', 'File system watching'],
    sectionId: 'behavior-file-system-watching',
    label: 'settings-behavior-file-system-watching',
  },
  { path: ['Behavior', 'Search'], sectionId: 'behavior-search', label: 'settings-behavior-search' },
  { path: ['AI'], sectionId: 'ai', label: 'settings-ai' },
  {
    path: ['File systems', 'SMB/Network shares'],
    sectionId: 'file-systems-smb-network-shares',
    label: 'settings-file-systems-smb',
  },
  {
    path: ['File systems', 'MTP (Android/Kindle/cameras)'],
    sectionId: 'file-systems-mtp-android-kindle-cameras',
    label: 'settings-file-systems-mtp',
  },
  { path: ['File systems', 'Git'], sectionId: 'file-systems-git', label: 'settings-file-systems-git' },
  { path: ['Viewer'], sectionId: 'viewer', label: 'settings-viewer' },
  { path: ['Keyboard shortcuts'], sectionId: 'keyboard-shortcuts', label: 'settings-keyboard-shortcuts' },
  { path: ['Developer', 'MCP server'], sectionId: 'developer-mcp-server', label: 'settings-developer-mcp-server' },
  { path: ['Developer', 'Logging'], sectionId: 'developer-logging', label: 'settings-developer-logging' },
  { path: ['Updates & privacy'], sectionId: 'updates', label: 'settings-updates' },
  { path: ['License'], sectionId: 'license', label: 'settings-license' },
  { path: ['Advanced'], sectionId: 'advanced', label: 'settings-advanced' },
]

/**
 * Captures the Settings window's every section.
 *
 * Settings runs in its own Tauri WebviewWindow (own webview JS context, own
 * `__cmdrI18nCapture` sink). Open it ONCE, then drive the production
 * `navigate-to-section` deep-link (the same event the volume picker / shortcut
 * chips use, passing the English section PATH so subsections land on real
 * content, not a parent summary grid) to each section, reusing the one window +
 * sink. Each `captureSurface` re-focuses for the shot so macOS composites the
 * current frame into the backing store the native capture reads.
 *
 * Wrapped so an open failure marks every not-yet-done settings surface failed
 * (rather than throwing) and the window is always closed. Per-surface isolation
 * means one section's failure doesn't stop the rest.
 */
export async function captureSettingsWindow(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  let settings: TauriPage | undefined
  try {
    settings = await openSettingsWindowViaProd(main)
    const settingsPage = settings
    await settingsPage.waitForSelector('.settings-window', 5000)
    // The settings page gates content behind `{#if initialized}`, which flips
    // true at the END of an async `onMount`. Focus the window so its async inits
    // aren't throttled while occluded, then wait for `initialized` (the sidebar
    // renders only after it). Don't wait on a specific section here: the
    // default-rendered section is restored from the persisted store and is
    // non-deterministic — the loop below deep-links to each section explicitly.
    await focusWindow(settingsPage, 'settings')
    await settingsPage.waitForSelector('.settings-window .section-item', 10000)
    await captureCall(settingsPage, 'reset')
    await captureCall<boolean>(settingsPage, 'enable')

    for (const section of SETTINGS_SECTIONS) {
      await captureSurface(section.label, report, failed, async () => {
        const sectionJson = JSON.stringify({ section: section.path })
        await settingsPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit_to', {
          target: { kind: 'AnyLabel', label: 'settings' },
          event: 'navigate-to-section',
          payload: ${sectionJson}
        })`)
        await settingsPage.waitForSelector(`[data-section-id="${section.sectionId}"]`, 5000)
        return { page: settingsPage, focusLabel: 'settings' }
      })
    }
    await captureCall(settingsPage, 'disable')
  } catch (err) {
    // Opening the window or the `initialized` wait failed: mark every
    // not-yet-done settings surface failed so the run continues and the report
    // stays honest.
    for (const { label } of SETTINGS_SECTIONS) {
      if (!(label in report) && !failed.includes(label)) failed.push(label)
    }
    console.warn(`[i18n-capture] Settings window setup FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    if (settings) await closeScopedWindow(main, settings, 'settings').catch(() => {})
  }
}

/**
 * Captures every MAIN-WINDOW overlay surface: the file-op dialogs (new file,
 * delete, trash, rename, extension-change, conflict, transfer), the navigation
 * and networking dialogs (go-to-path, connect-to-server), the command palette,
 * and the shared-`QueryDialog` query UI (search, selection, filter popover).
 *
 * All render into the main window's own capture sink, so each follows the About
 * pattern: enable + setSurface the sink BEFORE opening (to record mount-time
 * `t()` calls), open, wait on a per-overlay selector, capture, then dismiss +
 * disable. The `mainOverlay` local wraps that rhythm; its `open` callback does
 * the surface-specific staging and returns the wait selector. Surfaces that
 * mutate the fixture tree (rename, delete, conflict) get a fresh tree up front,
 * and cursor-dependent stages re-skip the synthetic `..` row first.
 *
 * Extracted from the test body so each surface's staging stays small and the
 * driver's top-level complexity stays under the lint ceiling.
 */
export async function captureMainOverlays(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await initMcpClient(main)

  // Stages one main-window overlay: enable+setSurface, run the surface-specific
  // `open` (returns its wait selector), let `captureSurface` shoot it, then
  // dismiss + disable. `dismissOverlay` no-ops (caught) for non-overlay surfaces.
  const mainOverlay = async (label: string, open: () => Promise<string>): Promise<void> => {
    await captureSurface(label, report, failed, async () => {
      await captureCall(main, 'reset')
      await captureCall(main, 'setSurface', label)
      await captureCall<boolean>(main, 'enable')
      const waitSelector = await open()
      await main.waitForSelector(waitSelector, 5000)
      return { page: main }
    })
    await dismissOverlay(main).catch(() => {})
    await captureCall(main, 'disable').catch(() => {})
  }

  // New-file dialog (⇧F4 → `file.newFile`). The mkfile twin of `new-folder-dialog`.
  await mainOverlay('new-file-dialog', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.newFile')
    return '[data-dialog-id="new-file-confirmation"] .name-input'
  })

  // Delete confirmation (F8 → `file.delete`): the recycle/trash-style confirm.
  await mainOverlay('delete-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.delete')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // Permanent-delete confirmation (⇧F8 → `file.deletePermanently`). Same
  // `delete-confirmation` dialog id as trash, but distinct copy (no-trash
  // warning / permanent wording), so it earns its own surface for the keys the
  // trash variant doesn't render.
  await mainOverlay('trash-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.deletePermanently')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // Rename: the inline editor (F2 → `file.rename`), NOT a modal — the input
  // mounts in-pane, so `dismissOverlay` (which only knows overlay selectors)
  // can't close it. Cancel the editor explicitly with a synthetic Escape.
  await captureSurface('rename-dialog', report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', 'rename-dialog')
    await captureCall<boolean>(main, 'enable')
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.rename')
    await main.waitForSelector('.rename-input', 5000)
    return { page: main }
  })
  await main.evaluate(`(function(){
    var el = document.querySelector('.rename-input');
    if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  })()`)
  await expect.poll(async () => (await main.count('.rename-input')) === 0, { timeout: 3000 }).toBeTruthy()
  await captureCall(main, 'disable').catch(() => {})

  // Extension-change confirmation: rename to a MEANINGFULLY different extension
  // (default `fileOperations.allowFileExtensionChanges` is "ask"). `.txt` → a
  // non-equivalent extension like `.zip` triggers the dialog; equivalent groups
  // (`.txt`/`.md`, `.jpg`/`.jpeg`, …) are silently allowed and would NOT show it.
  // Drive the inline editor to the new extension, then ⏎.
  await mainOverlay('extension-change', async () => {
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-a.txt')
    await dispatchMenuCommand(main, 'file.rename')
    await main.waitForSelector('.rename-input', 3000)
    await main.evaluate(`(function(){
      var el = document.querySelector('.rename-input');
      if (!el) return;
      el.focus();
      el.value = 'file-a.zip';
      el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
    await main.evaluate(`(function(){
      var el = document.querySelector('.rename-input');
      if (el) el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    })()`)
    return '[data-dialog-id="extension-change"]'
  })

  // Conflict-resolution dialog: the inline `.conflict-section` inside the
  // transfer-progress dialog. Stage a same-name collision (a `file-a.txt` in
  // `right/`), copy `file-a.txt` left→right under the "Ask for each" (stop)
  // policy so the per-file conflict prompt opens rather than an upfront policy.
  await mainOverlay('conflict-dialog', async () => {
    // Recreate first: an earlier surface (extension-change cancel) may leave the
    // tree perturbed, and we need `file-a.txt` present on both sides to collide.
    recreateFixtures(getFixtureRoot())
    writeFile(getFixtureRoot(), 'right/file-a.txt', 'dest-collision')
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-a.txt')
    await dispatchMenuCommand(main, 'file.copy')
    await main.waitForSelector(TRANSFER_DIALOG, 5000)
    await waitForConflictPolicy(main)
    await clickTransferStart(main)
    await main.waitForSelector('[data-dialog-id="transfer-progress"]', 3000)
    return '.conflict-section'
  })
  // The conflict flow opens the transfer-progress modal; cancel it cleanly so it
  // doesn't run the copy or leak into later surfaces.
  await dismissOverlay(main).catch(() => {})

  // Go-to-path dialog (`nav.goToPath`).
  await mainOverlay('go-to-path', async () => {
    await dispatchMenuCommand(main, 'nav.goToPath')
    return '[data-dialog-id="go-to-path"] input[aria-label="Path to go to"]'
  })

  // Copy/move transfer dialog (F5 → `file.copy`): the source→dest picker with the
  // operation toggle and counters, BEFORE confirming. No collision here, so it
  // shows the plain confirm state (distinct from the conflict surface above).
  await mainOverlay('transfer-dialog', async () => {
    // Recreate first: the conflict surface left a collision in `right/`; a clean
    // tree gives the plain confirm state (no upfront conflict-policy block).
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-b.txt')
    await dispatchMenuCommand(main, 'file.copy')
    return TRANSFER_DIALOG
  })

  // Command palette (`app.commandPalette`).
  await mainOverlay('command-palette', async () => {
    await dispatchMenuCommand(main, 'app.commandPalette')
    return '.palette-overlay .search-input'
  })

  // Search dialog (`search.open`). Shares the `.search-overlay` markup with the
  // selection dialog; captured FIRST so search-specific keys couple here and the
  // selection dialog below only claims its remaining unique keys.
  await mainOverlay('search-dialog', async () => {
    await dispatchMenuCommand(main, 'search.open')
    return '.search-overlay .query-input'
  })

  // Filter-chip popover: open the Search dialog, then the Size filter chip's
  // popover, for the chip/popover copy. The search dialog was torn down above,
  // so re-open it here.
  await mainOverlay('filter-popover', async () => {
    await dispatchMenuCommand(main, 'search.open')
    await main.waitForSelector('.search-overlay', 5000)
    await main.click('.search-overlay .chip-filter[aria-label="Size"]')
    return '.search-overlay .ui-popover'
  })
  // The popover sits ON the search dialog; dismiss both (popover first).
  await dismissOverlay(main).catch(() => {})

  // Selection dialog (`selection.selectFiles`): the "Select files…" twin of the
  // search dialog (same `QueryDialog` markup), so most keys already coupled to
  // `search-dialog`; this claims the selection-only ones.
  await mainOverlay('select-dialog', async () => {
    await dispatchMenuCommand(main, 'selection.selectFiles')
    return '.search-overlay .query-input'
  })

  // Connect-to-server dialog: reachable from the Network volume's browser via the
  // "+ Connect to server…" pseudo-row. Switch the left pane to Network (MCP),
  // then double-click the connect row (a single click only moves the cursor onto
  // it; `handleConnectRowDoubleClick` is what opens the dialog).
  await mainOverlay('connect-to-server', async () => {
    await mcpSelectVolume('left', 'Network')
    await main.waitForSelector('.network-browser .connect-row', 10000)
    await main.evaluate(`(function(){
      var el = document.querySelector('.network-browser .connect-row');
      if (el) el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    })()`)
    return '[data-dialog-id="connect-to-server"]'
  })
  // Leave the panes back on local so nothing downstream inherits Network.
  await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
}

/**
 * Captures the SNAPSHOT-RESOLVED toast surfaces (command-handler confirmations
 * and the transfer-completion toast). Each follows the `captureToastSurface`
 * rhythm: enable the sink, fire the trigger, catch the toast. Triggers are the
 * same registry commands / file ops the production UI uses.
 *
 * Toasts that need backend events we can't fire from the frontend (the real
 * download-complete toast, MTP-connected, low-disk) are NOT here — they're
 * documented skips deferred to the mock-staged surfaces.
 */
export async function captureFrontendToasts(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await initMcpClient(main)

  // Favorite the focused pane's folder (`favorites.add` → success toast). The
  // fixture root isn't favorited yet, so this hits the success path.
  await captureToastSurface('toast-favorite', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'favorites.add')
  })

  // Reopen-closed-tab with empty history (`tab.reopen` → "no recently closed
  // tabs" warning). At app start nothing has been closed, so the empty branch
  // fires. The success path emits no toast, so this is the reachable tab toast.
  await captureToastSurface('toast-tab', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'tab.reopen')
  })

  // Zoom in (`view.zoom.in` → "Zoom increased to N%" + reset-hint). Resolves
  // `commands.handler.zoomIncreased` plus a `zoomResetHint*` key.
  await captureToastSurface('toast-zoom-in', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'view.zoom.in')
  })

  // Zoom reset to 100% (`view.zoom.set100` → `commands.handler.zoomReset`). Also
  // restores the global text size that `toast-zoom-in` bumped, so later surfaces
  // render at the default scale.
  await captureToastSurface('toast-zoom-reset', report, failed, main, async () => {
    await dispatchMenuCommand(main, 'view.zoom.set100')
  })

  // Transfer-complete toast: finish a real copy and catch the completion toast
  // (`transfer.split.clean` for a clean single-file copy). Recreate first so the
  // copy lands in a clean `right/` with no collision (a conflict would change the
  // flow). Copy `file-b.txt` left→right and confirm.
  await captureToastSurface('toast-transfer-complete', report, failed, main, async () => {
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-b.txt')
    await dispatchMenuCommand(main, 'file.copy')
    await main.waitForSelector(`${TRANSFER_DIALOG} .btn-primary`, 5000)
    await main.click(`${TRANSFER_DIALOG} .btn-primary`)
  })
}

/**
 * Captures the empty-directory pane messaging (`fileExplorer.list.empty`).
 *
 * The fixture `right/` starts empty. Focus the right pane and navigate it there
 * via the production `mcp-nav-to-path` event (the same path the MCP nav tool
 * uses); the list view renders `.empty-folder-message` when the directory has no
 * entries. Mounted markup, so the normal `captureSurface` rerender path records
 * its keys — no snapshot-before-trigger needed.
 */
export async function captureEmptyPane(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await captureSurface('empty-pane', report, failed, async () => {
    // Make a guaranteed-fresh empty directory under the fixture root and navigate
    // a pane into it via the MCP `nav_to_path` tool (which acks on completion, so
    // we know the listing actually swapped — `mcp-nav-to-path` is fire-and-forget
    // and silently no-ops on a same-path or non-local pane). A brand-new dir
    // (not the start-path `right/`, which a prior copy or cached listing can leave
    // non-empty) reliably forces a re-listing of an empty directory. Navigate the
    // LEFT (focused) pane so the empty state sits in the visible, focused pane.
    // The list view renders `.empty-folder-message` when the directory is empty.
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(main)
    await initMcpClient(main)
    const emptyDir = join(getFixtureRoot(), 'empty-for-capture')
    mkdirSync(emptyDir, { recursive: true })
    await mcpNavToPath('left', emptyDir)
    await mcpAwaitPath('left', 'empty-for-capture')
    await main.waitForSelector('.empty-folder-message', 5000)
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})
}

/** Loop bound for the onboarding step walk (a couple over the step count so a no-op click can't spin). */
const ONBOARDING_STEP_BOUND = 6

/**
 * Captures the onboarding wizard, ONE surface per step.
 *
 * Each step (`StepFda` / `StepAi` / `StepBeta` / `StepOptional`) is a separate
 * component rendered into `.wizard-body` by the step cursor, so a step's `t()` /
 * `<Trans>` keys only mount while that step is active. The wizard lives in the
 * MAIN window's sink (it's an in-app sheet, not a separate window), and it's
 * mounted markup — the normal rerender path records the active step's keys. So
 * for each step: setSurface, rerender, screenshot, then click the forward button
 * to advance.
 *
 * Staging: `cmdr.openOnboarding` opens the wizard at the first reachable step. On
 * macOS the E2E fixture grants FDA, so step 1 shows the `already-granted`
 * (single-Next) variant; the other FDA variants need per-launch `CMDR_MOCK_FDA`
 * the shared instance can't supply, so they stay covered by the tier-3 Vitest
 * specs (documented in `onboarding.spec.ts`). Linux skips step 1.
 *
 * Advancing uses the wizard's own forward button (the last button in
 * `.primary-slot`), exactly as `onboarding.spec.ts` does. The final step's button
 * finishes onboarding and unmounts the wizard, which is the natural cleanup.
 */
export async function captureOnboardingWizard(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  const WIZARD = '[data-dialog-id="onboarding"]'
  // Surfaces per step, in step order. The macOS step-1 (FDA) surface is dropped
  // on Linux (no step 1 there); the loop reads the live active-step dot so it
  // labels whatever actually rendered.
  const stepLabels: Record<number, string> = {
    1: 'onboarding-fda',
    2: 'onboarding-ai',
    3: 'onboarding-beta',
    4: 'onboarding-optional',
  }

  const activeStep = async (): Promise<number | null> =>
    main.evaluate<number | null>(`(function(){
      var dots = document.querySelectorAll('${WIZARD} .step-dot');
      for (var i = 0; i < dots.length; i++) {
        if (dots[i].getAttribute('aria-current') === 'step') return i + 1;
      }
      return null;
    })()`)

  const clickForward = async (): Promise<void> => {
    await main.evaluate(`(function(){
      var btns = document.querySelectorAll('${WIZARD} .primary-slot button');
      if (btns.length > 0) btns[btns.length - 1].click();
    })()`)
  }

  let opened = false
  try {
    await ensureAppReady(main)
    await dispatchMenuCommand(main, 'cmdr.openOnboarding')
    await main.waitForSelector(WIZARD, 5000)
    opened = true
    // Enable the sink ONCE for the whole wizard; each step re-`setSurface`s under
    // its own label so the coupler assigns each step's keys to that step.
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')

    // Walk the steps. Cap the loop above the step count so a no-op click can't
    // spin. Each iteration captures the live step, then advances.
    for (let i = 0; i < ONBOARDING_STEP_BOUND; i++) {
      if (!(await main.isVisible(WIZARD))) break
      const step = await activeStep()
      if (step === null) break
      const label = stepLabels[step] ?? `onboarding-step-${String(step)}`
      if (!(label in report)) {
        await captureSurface(label, report, failed, async () => {
          await main.waitForSelector(`${WIZARD} .step-shell`, 5000)
          await captureCall(main, 'setSurface', label)
          await captureCall(main, 'rerender')
          return { page: main }
        })
      }
      // Advance to the next step; the final step's button finishes + unmounts.
      const before = step
      await clickForward()
      await expect
        .poll(async () => !(await main.isVisible(WIZARD)) || (await activeStep()) !== before, { timeout: 5000 })
        .toBeTruthy()
    }
    await captureCall(main, 'disable').catch(() => {})
  } catch (err) {
    for (const label of Object.values(stepLabels)) {
      if (!(label in report) && !failed.includes(label)) failed.push(label)
    }
    console.warn(`[i18n-capture] onboarding setup FAILED: ${err instanceof Error ? err.message : String(err)}`)
  } finally {
    // If the walk didn't finish (a capture threw mid-flow), make sure the wizard
    // is closed so it doesn't leak into later surfaces. Best-effort: advance to
    // the end. `opened` guards the no-op case.
    if (opened) {
      for (let i = 0; i < ONBOARDING_STEP_BOUND; i++) {
        if (!(await main.isVisible(WIZARD).catch(() => false))) break
        await clickForward().catch(() => {})
        await expect
          .poll(
            async () =>
              (await main.count(WIZARD).catch(() => 1)) === 0 || !(await main.isVisible(WIZARD).catch(() => false)),
            {
              timeout: 1500,
            },
          )
          .toBeTruthy()
          .catch(() => {})
      }
    }
  }
}

/**
 * Captures the "What's new" post-update popup (`whatsNew.*`).
 *
 * The boot auto-check is suppressed under E2E, so this drives the same real path
 * the `whats-new.spec.ts` test uses: emit the E2E-gated `e2e-rerun-whats-new`
 * event (seeds `isOnboarded` + an old `lastSeenVersion`, force-runs
 * `maybeRunWhatsNew`), which opens the dialog with a non-empty release slice.
 * Mounted markup, so the rerender path records its keys. The dialog dismisses via
 * the footer Close button.
 */
export async function captureWhatsNew(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await captureSurface('whats-new', report, failed, async () => {
    await ensureAppReady(main)
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'e2e-rerun-whats-new',
      payload: { isOnboarded: true, lastSeenVersion: '0.1.0', showOnUpdate: true }
    })`)
    await main.waitForSelector('#whats-new-body', 5000)
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})
  // Close via the footer's last button (Close); the opt-out button would flip the
  // showOnUpdate setting, which we don't want to mutate. dismissOverlay can't
  // close it (the dialog isn't in the OVERLAY_SELECTORS set).
  await main
    .evaluate(`(function(){
      var btns = document.querySelectorAll('#whats-new-body .footer .btn');
      if (btns.length > 0) btns[btns.length - 1].click();
    })()`)
    .catch(() => {})
  await expect.poll(async () => (await main.count('#whats-new-body')) === 0, { timeout: 3000 }).toBeTruthy()
}

/**
 * Captures the drive-indexing status indicator (`indexing.scan.*` /
 * `indexing.eta.*`).
 *
 * The indicator (`.indexing-status`) renders only while a scan / aggregation /
 * replay is live, driven by Tauri events the Rust indexer emits. We can't start a
 * real backend scan deterministically from the frontend, so we emit the same
 * typed events directly: an `index-scan-started` (with a prior-scan calibration
 * so the tier-1 percent + ETA render) followed by an `index-scan-progress`. The
 * frontend's `initIndexState` listeners flip `scanning` true and the indicator
 * mounts. Its labels resolve reactively via `tString`/`$derived`, so the rerender
 * path records them.
 *
 * This only covers the SCAN keys; aggregation and replay would need their own
 * event pairs. The rescan-notification TOAST (`indexing.rescan.*`) is a separate
 * snapshot toast deferred to the mock-staged surfaces (it needs a typed rescan
 * event with a reason).
 */
export async function captureIndexingStatus(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await captureSurface('indexing-status', report, failed, async () => {
    await ensureAppReady(main)
    // Emit a started + progress pair so the indicator mounts with a populated
    // tier-1 percent/ETA. `volumeId` is cosmetic for the indicator; the prior-*
    // fields drive the calibrated progress tier.
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'index-scan-started',
      payload: { volumeId: 'i18n-capture', priorTotalEntries: 100000, priorScanDurationMs: 30000, volumeUsedBytes: null }
    })`)
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'index-scan-progress',
      payload: { volumeId: 'i18n-capture', entriesScanned: 42000, dirsFound: 3500, bytesScanned: 0 }
    })`)
    await main.waitForSelector('.indexing-status', 5000)
    await captureCall(main, 'reset')
    await captureCall<boolean>(main, 'enable')
    return { page: main }
  })
  await captureCall(main, 'disable').catch(() => {})
  // Clear the faked scan state so the indicator unmounts and nothing downstream
  // inherits a stuck hourglass.
  await main
    .evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
      event: 'index-scan-complete',
      payload: { volumeId: 'i18n-capture', totalEntries: 100000, totalDirs: 3500, durationMs: 30000 }
    })`)
    .catch(() => {})
}
