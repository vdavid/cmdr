/**
 * The i18n capture's MAIN pass as data: every surface group, in coupling order.
 *
 * Two specs walk this one list. `i18n-capture.spec.ts` runs every step and
 * photographs each surface (`pnpm i18n:shots`). `i18n-capture-staging.spec.ts`
 * runs the steps in stage-only mode (`isStageOnly`) inside the routine E2E lane,
 * so a UI change that breaks the capture's staging fails at the commit that made
 * it. One list is what keeps the two from drifting: a step added here is staged by
 * the lane the same day, unless the staging spec excludes it with a reason.
 *
 * Coupling policy: a key may render on several surfaces, and the coupler assigns
 * each key the FIRST surface (in this list's order) it appeared on, so the most
 * specific surface wins when steps run narrow to broad. Keep the order
 * intentional.
 *
 * The license and FDA passes aren't here: each needs a launch of its own, which
 * only the orchestrator gives them (`runMockPass` in the capture spec).
 */

import { closeScopedWindow, dismissOverlay, dispatchMenuCommand, ensureAppReady, MKDIR_DIALOG } from './helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { type SurfaceEntry, captureCall, captureSurface, focusWindow, settlePaint } from './i18n-capture-helpers.js'
import { isOverflowPass, overflowLocale } from './i18n-capture-config.js'
import {
  captureSettingsWindow,
  captureMainOverlays,
  captureFrontendToasts,
  captureEmptyPane,
  captureOnboardingWizard,
  captureWhatsNew,
  captureIndexingStatus,
  captureIndexingGallery,
} from './i18n-capture-surfaces.js'
import {
  captureMainDialogs,
  captureViewerSubsurfaces,
  captureQueueWindow,
  captureOperationChipSurfaces,
} from './i18n-capture-special.js'
import { captureErrorPaneExample, captureMainExplorerSurfaces } from './i18n-capture-surfaces-main.js'
import { captureGalleryDialogs } from './i18n-capture-gallery.js'
import { captureAskCmdrSurfaces } from './i18n-capture-ask-cmdr.js'
import {
  captureMtpBrowse,
  captureMtpConnectedToast,
  captureDownloadToasts,
  captureQuickLookHint,
} from './i18n-capture-staged.js'

/** The ledgers every step appends to. */
export interface PassLedger {
  /** Surface label → its entry, for every surface the run finished. */
  report: Record<string, SurfaceEntry>
  /** Surfaces that threw unexpectedly. Any entry fails the run. */
  failed: string[]
  /** Surfaces deliberately left out as a documented gap. These don't fail the run. */
  skipped: string[]
}

/** One surface group of the main pass. */
export interface MainPassStep {
  /** Stable name: the staging spec's test title, and the key its exclusions use. */
  name: string
  /** Stages the group's surfaces and, outside a stage-only run, photographs them. */
  run: (main: TauriPage, ledger: PassLedger) => Promise<void>
}

/**
 * Switches `page`'s app to the pseudolocale for the overflow pass (no-op for the
 * normal English coupling pass). Every E2E build exposes `setLocale` on the
 * capture API; the catalog for `overflowLocale` was baked into the glob at build
 * time (the orchestrator generates `en-XA` before getting the binary). Doing it via this
 * frontend-only seam is identical to the live Language picker, so the captured UI
 * is what a user switching language would see.
 */
async function switchToOverflowLocaleIfNeeded(page: TauriPage): Promise<void> {
  if (!isOverflowPass) return
  await captureCall(page, 'setLocale', overflowLocale)
  await settlePaint(page)
}

export const MAIN_PASS_STEPS: readonly MainPassStep[] = [
  // The main dual-pane window. The overflow pass switches the whole app to the
  // pseudolocale here, BEFORE any surface is captured, so every surface renders in
  // the expanded, accented strings.
  {
    name: 'main-window',
    run: async (main, { report, failed }) => {
      await captureSurface('main-window', report, failed, async () => {
        await ensureAppReady(main)
        await main.waitForSelector('.file-entry', 5000)
        await switchToOverflowLocaleIfNeeded(main)
        await captureCall(main, 'reset')
        await captureCall<boolean>(main, 'enable')
        return { page: main }
      })
    },
  },

  // The new-folder dialog: a modal overlay on the main window, sharing the main
  // sink.
  //
  // Opened via the registry command (the `file.newFolder` twin of
  // `new-file-dialog`'s `file.newFile`), NOT a synthetic `F7` keypress. The Tauri
  // `execute-command` event path is unaffected by DOM focus, whereas a synthesized
  // keypress lands on `document.activeElement`, which is `<body>` whenever the E2E
  // main window has lost OS focus (its `Prohibited` activation policy and
  // ordered-to-back windows make that the norm), so the key never reaches the
  // explorer's keydown handler. Making a folder doesn't depend on the cursor, so no
  // `skipParentEntry` is needed either.
  {
    name: 'new-folder-dialog',
    run: async (main, { report, failed }) => {
      await captureSurface('new-folder-dialog', report, failed, async () => {
        await dispatchMenuCommand(main, 'file.newFolder')
        await main.waitForSelector(MKDIR_DIALOG, 5000)
        await main.waitForSelector(`${MKDIR_DIALOG} input.text-field-control`, 3000)
        return { page: main }
      })
      // `captureSurface` already isolated any staging failure (and recorded it in
      // `failed`). The cleanup must not itself throw when the dialog never opened:
      // `dismissOverlay`'s "no overlay is open" abort would skip every later step.
      await dismissOverlay(main).catch(() => {})
      await captureCall(main, 'disable').catch(() => {})
    },
  },

  // Main-window file-explorer states the dialog and window groups miss: a live
  // multi-file selection, the Shift fork of the function-key bar, and the pane's
  // volume chooser. All render into the main window's sink, so they run before the
  // separate-window groups pull focus away.
  {
    name: 'main-explorer',
    run: async (main, { report, failed }) => {
      await captureMainExplorerSurfaces(main, report, failed)
    },
  },

  // The Ask Cmdr rail (consent → empty → one exchange → threads). Also a
  // main-window panel, and early for one reason: the consent gate is a one-time
  // screen recorded in `main.db`, so the only chance to photograph it is before
  // anything in this run accepts it.
  {
    name: 'ask-cmdr',
    run: async (main, { report, failed, skipped }) => {
      await captureAskCmdrSurfaces(main, report, failed, skipped)
    },
  },

  // The Settings window, every section.
  {
    name: 'settings-window',
    run: async (main, { report, failed }) => {
      await captureSettingsWindow(main, report, failed)
    },
  },

  // Viewer subsurfaces (search, context menu, pickers). Each opens its own viewer
  // window (own webview context and sink) on a fixture file.
  //
  // ❌ There's no plain `viewer` surface any more. It photographed the default text
  // chrome, which every state below already shows, so it resolved not one key the
  // others don't. `viewer-search` runs first and takes the shared viewer keys; it's
  // also what the `viewer.` representative points at for the states nothing
  // captures.
  {
    name: 'viewer',
    run: async (main, { report, failed, skipped }) => {
      await captureViewerSubsurfaces(main, report, failed, skipped)
    },
  },

  // The About dialog: an in-app dialog rendered into the MAIN window (not a separate
  // window), opened via `app.about`. Re-enable and `setSurface` BEFORE opening so its
  // mount-time `t()` calls record under `about` too. Before the shortcuts window, so
  // that window's open/close can't perturb the main sink between the dialog mount
  // and its key dump.
  {
    name: 'about',
    run: async (main, { report, failed }) => {
      await captureSurface('about', report, failed, async () => {
        await captureCall(main, 'setSurface', 'about')
        await captureCall<boolean>(main, 'enable')
        await dispatchMenuCommand(main, 'app.about')
        await main.waitForSelector('[data-dialog-id="about"]', 5000)
        return { page: main }
      })
      await dismissOverlay(main).catch(() => {})
      await captureCall(main, 'disable').catch(() => {})
    },
  },

  // Main-window overlays: the file-operation dialogs, the palette, the query UI, and
  // the servers hub, each staged by a registry command.
  {
    name: 'main-overlays',
    run: async (main, { report, failed }) => {
      await captureMainOverlays(main, report, failed)
    },
  },

  // The license-key entry, error-report, feedback, and acknowledgements dialogs on
  // the default launch. The commercial and expired license surfaces need a
  // `CMDR_MOCK_LICENSE` launch (the license passes).
  {
    name: 'main-dialogs',
    run: async (main, { report, failed }) => {
      await captureMainDialogs(main, report, failed)
    },
  },

  // Snapshot-resolved toasts: command-handler confirmations and the
  // transfer-complete toast. They resolve their text ONCE at emit time, so the sink
  // is enabled BEFORE the action fires. After the dialogs, so dialog keys couple
  // narrow-first.
  {
    name: 'frontend-toasts',
    run: async (main, { report, failed }) => {
      await captureFrontendToasts(main, report, failed)
    },
  },

  // Empty-directory pane messaging.
  {
    name: 'empty-pane',
    run: async (main, { report, failed }) => {
      await captureEmptyPane(main, report, failed)
    },
  },

  // The onboarding wizard, one surface per step.
  {
    name: 'onboarding',
    run: async (main, { report, failed }) => {
      await captureOnboardingWizard(main, report, failed)
    },
  },

  // The what's-new post-update popup.
  {
    name: 'whats-new',
    run: async (main, { report, failed }) => {
      await captureWhatsNew(main, report, failed)
    },
  },

  // The drive-indexing checklist in every state, from the dev Graphics gallery's
  // fixtures. BEFORE the live indicator, so the gallery (which actually SHOWS the
  // checklist body) wins the shared `indexing.*` keys and the live indicator owns
  // only the hourglass's `indexing.status.ariaLabel`.
  {
    name: 'indexing-gallery',
    run: async (main, { report, failed }) => {
      await captureIndexingGallery(main, report, failed)
    },
  },

  // The live drive-indexing hourglass.
  {
    name: 'indexing-status',
    run: async (main, { report, failed }) => {
      await captureIndexingStatus(main, report, failed)
    },
  },

  // Mock-staged surfaces reachable in the default launch: the E2E binary carries
  // `virtual-mtp`, the store is hermetic and default, and the launch sets
  // `CMDR_MOCK_FDA`. The Quick Look hint fires on Space because the default store
  // doesn't suppress it; the MTP device auto-registers under E2E mode and its toast
  // re-fires from the typed connect event; the download toast comes from the
  // `download-detected` event with the FDA gate mocked open.
  {
    name: 'quick-look-hint',
    run: async (main, { report, failed }) => {
      await captureQuickLookHint(main, report, failed)
    },
  },
  {
    name: 'mtp-browse',
    run: async (main, { report, failed }) => {
      await captureMtpBrowse(main, report, failed)
    },
  },
  {
    name: 'mtp-connected-toast',
    run: async (main, { report, failed }) => {
      await captureMtpConnectedToast(main, report, failed)
    },
  },
  {
    name: 'download-toast',
    run: async (main, { report, failed }) => {
      await captureDownloadToasts(main, report, failed)
    },
  },

  // One real friendly-error pane, the representative image for the whole `errors.*`
  // family (`REPRESENTATIVE_SCREENSHOTS` in `scripts/couple-screenshots.ts`), staged
  // through the `inject_listing_error` E2E hook. Last among the hand-staged main
  // window surfaces, so a transient error state can't perturb earlier ones.
  {
    name: 'error-pane',
    run: async (main, { report, failed }) => {
      await captureErrorPaneExample('error-message-example', report, failed, main)
    },
  },

  // Registry-driven soft dialogs: every gallery state worth a shot, through the main
  // window's own `debug-open-gallery-dialog` listener. LAST among the main-window
  // groups: a state is photographed only when it resolves a key nothing else
  // recorded, so a faithful capture of the production path always beats a gallery
  // preview of the same dialog. `i18n-capture-gallery.ts` carries its two limits.
  {
    name: 'gallery-dialogs',
    run: async (main, { report, failed, skipped }) => {
      await captureGalleryDialogs(main, report, failed, skipped)
    },
  },

  // The standalone Keyboard shortcuts window (label `shortcuts`, opened by
  // `help.openShortcuts`): its own webview context and sink.
  //
  // Every eval into a secondary window rides back on the plugin's own
  // `plugin:playwright|pw_result` IPC, which Tauri gates per window through the
  // capability ACL. A window missing from the E2E-only `playwright.json` capability
  // (generated by `src-tauri/build.rs`) can never post its result, so even `1+1`
  // burns the plugin's 30 s ceiling and looks like a hang. Keep this window (and
  // `queue`) in that list.
  {
    name: 'shortcuts-window',
    run: async (main, { report, failed }) => {
      let shortcuts: TauriPage | undefined
      await captureSurface('shortcuts', report, failed, async () => {
        await dispatchMenuCommand(main, 'help.openShortcuts')
        shortcuts = await main.waitForWindow((w) => w.label === 'shortcuts', { timeout: 10000 })
        const s = shortcuts
        // `waitForWindow` returns the moment the label exists, which is BEFORE the
        // document loads; an eval landing in the outgoing document is torn down
        // before it can post its result, so one retry rides that out.
        await s.waitForSelector('.shortcuts-scroll .row', 15000).catch(async () => {
          await s.waitForSelector('.shortcuts-scroll .row', 15000)
        })
        await focusWindow(s, 'shortcuts')
        await captureCall(s, 'reset')
        await captureCall<boolean>(s, 'enable')
        return { page: s, focusLabel: 'shortcuts' }
      })
      if (shortcuts) await closeScopedWindow(main, shortcuts, 'shortcuts').catch(() => {})
    },
  },

  // The operation-queue window (empty, populated, failed): its own window on
  // `/queue`, and the only place a queue ROW renders.
  {
    name: 'queue-window',
    run: async (main, { report, failed }) => {
      await captureQueueWindow(main, report, failed)
    },
  },

  // The main window's corner chip and failure notice: the other half of `queue.*`,
  // which exists only while work is in flight. After the queue window closes, so the
  // main window is what's on screen, and last overall because a retained failure is
  // sticky until something explicitly dismisses it.
  {
    name: 'operation-chip',
    run: async (main, { report, failed }) => {
      await captureOperationChipSurfaces(main, report, failed)
    },
  },
]
