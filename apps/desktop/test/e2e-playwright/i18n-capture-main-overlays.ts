/**
 * Main-window overlay captures for the i18n screenshot-capture driver: the
 * file-operation dialogs, the conflict and transfer dialogs, go-to-path, the
 * command palette, the shared `QueryDialog` query UI, and the servers hub.
 *
 * All render into the main window's own capture sink, so each follows the About
 * pattern: enable + setSurface the sink BEFORE opening (to record mount-time `t()`
 * calls), open, wait on a per-overlay selector, capture, then dismiss + disable.
 *
 * One exported function per step of `MAIN_PASS_STEPS`, in coupling order, so no
 * single step carries the whole group (each step is a test of its own in the E2E
 * lane's stage-only run, where every test has a 2 s duration budget). The group's
 * full reset (fixture rebuild + `ensureAppReady`) runs ONCE, in the first step;
 * later steps state only the preconditions they add, because the steps always run
 * in this order.
 */

import {
  ensureAppReady,
  dismissOverlay,
  skipParentEntry,
  moveCursorToFile,
  dispatchMenuCommand,
  getFixtureRoot,
  LOCAL_VOLUME_NAME,
  TRANSFER_DIALOG,
} from './helpers.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { initMcpClient, mcpSelectVolume } from '../e2e-shared/mcp-client.js'
import { writeFile, waitForConflictPolicy, clickTransferStart } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface } from './i18n-capture-helpers.js'
import { resetOperationStateOrReport } from './i18n-capture-operations.js'

/**
 * Stages and captures one main-window overlay: enable + setSurface, run the
 * surface-specific `open` (which returns its wait selector), and let
 * `captureSurface` shoot it. The wait selector doubles as the `readySelector`, so
 * the same condition is re-proven in the frame that gets photographed, not just at
 * open time. Leaves the overlay up; `captureMainOverlay` is the usual way out.
 */
async function stageMainOverlay(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  label: string,
  open: () => Promise<string>,
): Promise<void> {
  await captureSurface(label, report, failed, async () => {
    await captureCall(main, 'reset')
    await captureCall(main, 'setSurface', label)
    await captureCall<boolean>(main, 'enable')
    const waitSelector = await open()
    await main.waitForSelector(waitSelector, 5000)
    return { page: main, readySelector: waitSelector }
  })
}

/**
 * `stageMainOverlay`, then dismiss + disable. `dismissOverlay` no-ops (caught) for
 * a surface whose overlay never opened.
 */
async function captureMainOverlay(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
  label: string,
  open: () => Promise<string>,
): Promise<void> {
  await stageMainOverlay(main, report, failed, label, open)
  await dismissOverlay(main).catch(() => {})
  await captureCall(main, 'disable').catch(() => {})
}

/**
 * The confirmation dialogs a file operation opens on the file under the cursor:
 * new file, delete, permanent delete, and extension change. Starts from a fresh
 * fixture tree, since the rename and delete stages act on real files.
 */
export async function captureFileOperationDialogs(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  recreateFixtures(getFixtureRoot())
  await ensureAppReady(main)
  await initMcpClient(main)

  // New-file dialog (⇧F4 → `file.newFile`). The mkfile twin of `new-folder-dialog`.
  await captureMainOverlay(main, report, failed, 'new-file-dialog', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.newFile')
    return '[data-dialog-id="new-file-confirmation"] input.text-field-control'
  })

  // Delete confirmation (F8 → `file.delete`): the recycle/trash-style confirm.
  await captureMainOverlay(main, report, failed, 'delete-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.delete')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // Permanent-delete confirmation (⇧F8 → `file.deletePermanently`). Same
  // `delete-confirmation` dialog id as trash, but distinct copy (no-trash warning /
  // permanent wording), so it earns its own surface for the keys the trash variant
  // doesn't render.
  await captureMainOverlay(main, report, failed, 'trash-confirm', async () => {
    await skipParentEntry(main)
    await dispatchMenuCommand(main, 'file.deletePermanently')
    return '[data-dialog-id="delete-confirmation"]'
  })

  // ❌ No `rename-dialog` surface. The inline rename editor renders no copy of its
  // own (it's a bare input in the pane), so every key it recorded belonged to the
  // chrome around it and every one of them is captured elsewhere. The
  // `extension-change` surface below reaches the same editor and shows the dialog
  // that actually has words in it.

  // Extension-change confirmation: rename to a MEANINGFULLY different extension
  // (default `fileOperations.allowFileExtensionChanges` is "ask"). `.txt` → a
  // non-equivalent extension like `.zip` triggers the dialog; equivalent groups
  // (`.txt`/`.md`, `.jpg`/`.jpeg`, …) are silently allowed and would NOT show it.
  // Drive the inline editor to the new extension, then ⏎.
  await captureMainOverlay(main, report, failed, 'extension-change', async () => {
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
}

/**
 * The conflict-resolution dialog: the inline `.conflict-section` inside the
 * transfer-progress dialog. Stages a same-name collision (a `file-a.txt` in
 * `right/`) and copies `file-a.txt` left→right under the "Ask for each" (stop)
 * policy, so the per-file conflict prompt opens rather than an upfront policy.
 */
export async function captureConflictDialog(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await stageMainOverlay(main, report, failed, 'conflict-dialog', async () => {
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
  // The conflict flow leaves a real copy PARKED on an unanswered clash, and the
  // way out of one is an answer, never an Escape: `TransferProgressDialog` passes
  // `onclose={undefined}` while a clash is up, because every exit from a clash
  // decides something about the user's files. So the exit here is a real cancel on
  // the operation, which ends it and takes the dialog down with it, and it comes
  // FIRST: an Escape while the clash is up does nothing but wait out
  // `dismissOverlay`'s close poll, which cost this step ~3 s.
  //
  // ❗ Assert the drain, don't hope for it. A copy parked on a clash holds the
  // local lane AND a queue row, so leaving one behind fails every later surface
  // that needs real work in flight — `toast-transfer-complete` and all three queue
  // shots — dozens of surfaces downstream of the one that caused it. That is
  // exactly what a swallowed `dismissOverlay` failure did here once.
  await resetOperationStateOrReport(main, failed, 'conflict-dialog')
  await dismissOverlay(main).catch(() => {})
  await captureCall(main, 'disable').catch(() => {})
}

/** The go-to-path dialog (`nav.goToPath`). */
export async function captureGoToPath(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await captureMainOverlay(main, report, failed, 'go-to-path', async () => {
    await dispatchMenuCommand(main, 'nav.goToPath')
    // Stable class, NOT the `aria-label` text: the overflow pass pseudolocalizes
    // every label, so an English `aria-label="Path to go to"` selector never
    // matches under en-XA.
    return '[data-dialog-id="go-to-path"] input.text-field-control'
  })
}

/**
 * The copy/move transfer dialog (F5 → `file.copy`): the source→dest picker with
 * the operation toggle and counters, BEFORE confirming. No collision here, so it
 * shows the plain confirm state (distinct from the conflict surface).
 */
export async function captureTransferDialog(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await captureMainOverlay(main, report, failed, 'transfer-dialog', async () => {
    // Recreate first: the conflict surface left a collision in `right/`; a clean
    // tree gives the plain confirm state (no upfront conflict-policy block).
    recreateFixtures(getFixtureRoot())
    await ensureAppReady(main)
    await skipParentEntry(main)
    await moveCursorToFile(main, 'file-b.txt')
    await dispatchMenuCommand(main, 'file.copy')
    return TRANSFER_DIALOG
  })
}

/**
 * The command palette and the shared-`QueryDialog` query UI: the search dialog,
 * its filter-chip popover, and the selection dialog.
 */
export async function captureQueryOverlays(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  // Command palette (`app.commandPalette`).
  await captureMainOverlay(main, report, failed, 'command-palette', async () => {
    await dispatchMenuCommand(main, 'app.commandPalette')
    return '.palette-overlay input.text-field-control'
  })

  // Search dialog (`search.open`). Shares the `.search-overlay` markup with the
  // selection dialog; captured FIRST so search-specific keys couple here and the
  // selection dialog below only claims its remaining unique keys.
  await captureMainOverlay(main, report, failed, 'search-dialog', async () => {
    await dispatchMenuCommand(main, 'search.open')
    return '.search-overlay .query-bar input.text-field-control'
  })

  // Filter-chip popover: open the Search dialog, then the Size filter chip's
  // popover, for the chip/popover copy. The search dialog was torn down above,
  // so re-open it here.
  await captureMainOverlay(main, report, failed, 'filter-popover', async () => {
    await dispatchMenuCommand(main, 'search.open')
    await main.waitForSelector('.search-overlay', 5000)
    // Open the Size popover via its production shortcut (Option+S, matched on the
    // layout-stable `event.code === 'KeyS'`), NOT by clicking a chip selected on
    // its English `aria-label="Size"`: the overflow pass pseudolocalizes that
    // label, so the text selector never matches under en-XA.
    await main.evaluate(
      `document.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyS', key: 's', altKey: true, bubbles: true }))`,
    )
    return '.search-overlay .ui-popover'
  })
  // The popover sits ON the search dialog; dismiss both (popover first).
  await dismissOverlay(main).catch(() => {})

  // Selection dialog (`selection.selectFiles`): the "Select files…" twin of the
  // search dialog (same `QueryDialog` markup), so most keys already coupled to
  // `search-dialog`; this claims the selection-only ones.
  await captureMainOverlay(main, report, failed, 'select-dialog', async () => {
    await dispatchMenuCommand(main, 'selection.selectFiles')
    return '.search-overlay .query-bar input.text-field-control'
  })
}

/**
 * The servers hub: the table the Network row opens (`servers.hub.*`), whose last
 * row is always "Add server…", so it renders with no server saved or nearby. The
 * sheet that row opens is the gallery's `server-sign-in-*` states, so this surface
 * is the hub alone. Same 15 s budget `servers.spec.ts` gives the mount.
 */
export async function captureServersHub(
  main: TauriPage,
  report: Record<string, SurfaceEntry>,
  failed: string[],
): Promise<void> {
  await initMcpClient(main)
  await captureMainOverlay(main, report, failed, 'servers-hub', async () => {
    await mcpSelectVolume('left', 'Servers')
    await main.waitForSelector('.servers-hub .add-row', 15000)
    return '.servers-hub .add-row'
  })
  // Leave the panes back on local so nothing downstream inherits Network.
  await mcpSelectVolume('left', LOCAL_VOLUME_NAME).catch(() => {})
}
