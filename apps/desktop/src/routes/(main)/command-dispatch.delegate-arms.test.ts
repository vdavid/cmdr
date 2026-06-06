/**
 * Characterization suite for `handleCommandExecute`'s simple-delegate arms.
 *
 * The table-driven half of the dispatch characterization: ~80 one-call delegate
 * arms (each dispatchable id's exact explorer-method / dialog-callback + args),
 * extracted from `command-dispatch.characterization.test.ts` to keep both files
 * a readable length. The bespoke branches (zoom/tab toasts, activeElement input
 * branches, await semantics, the preamble order, the exempt no-op, and the
 * completeness self-checks) stay in the sibling file.
 *
 * The `DELEGATE_ROWS` table + the `makeCtx` / `makeExplorerSpy` builders are
 * imported from `command-dispatch.test-harness.ts`.
 *
 * ⚠️ The `vi.mock(...)` block below is INTENTIONALLY DUPLICATED with the sibling
 * `command-dispatch.characterization.test.ts`. Vitest hoists `vi.mock` PER TEST
 * FILE — a factory declared in the imported harness would NOT be hoisted into
 * this file's module graph, so importing `command-dispatch.ts` here would pull
 * the real deps. Each test file therefore carries its own full mock block.
 */
import { describe, it, vi, beforeEach } from 'vitest'

// --- Mocks (hoisted) -------------------------------------------------------
// All captured spies live in ONE `vi.hoisted` block so the hoisted `vi.mock`
// factories below can close over them safely. A plain top-level `const spy =
// vi.fn()` isn't initialized yet when a hoisted factory runs during the import
// of `command-dispatch.ts` (and its transitive deps), which throws "Cannot
// access X before initialization". `vi.hoisted` is evaluated before any mock
// factory, so the references are live.
const m = vi.hoisted(() => ({
  logInfo: vi.fn<(...a: unknown[]) => void>(),
  invoke: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  getVolumeId: vi.fn<() => string>(() => 'local'),
  getPanePath: vi.fn<() => string>(() => '/Users/test'),
  addToast: vi.fn<(...a: unknown[]) => void>(),
  getSetting: vi.fn<(key: string) => number>(() => 100),
  setSetting: vi.fn<(...a: unknown[]) => void>(),
  getEffectiveShortcuts: vi.fn<(id: string) => string[]>(() => []),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  openErrorReportDialog: vi.fn<() => void>(),
  runMenuTriggeredCheck: vi.fn(() => Promise.resolve()),
  goToLatestDownload: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  openExternalUrl: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  showInFinder: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  copyToClipboard: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookOpen: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookClose: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  getInfo: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  openInEditor: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  syncMenuShowHidden: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  readClipboardText: vi.fn<() => Promise<string>>(() => Promise.resolve('')),
  cloudMakeAvailableOffline: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  cloudRemoveDownload: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookState: { isOpen: false },
  quickLookDispatchGuardJustFired: vi.fn<() => boolean>(() => false),
  armQuickLookDispatchGuard: vi.fn<() => void>(),
}))

const {
  getVolumeId,
  getPanePath,
  getSetting,
  getEffectiveShortcuts,
  readClipboardText,
  quickLookDispatchGuardJustFired,
  quickLookState,
} = m

// `getAppLogger('user-action')` runs at module top-level; `m.logInfo` captures
// the logged `id` so the preamble-order + exempt tests can assert on it.
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: m.logInfo, debug: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

// The dispatch preamble fires `record_breadcrumb` via `invoke`.
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => m.invoke(...args) }))

// The capability guard reads the focused pane's volume id + path.
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: () => m.getVolumeId(),
  getFocusedPanePath: () => m.getPanePath(),
}))

// Empty store ⇒ `local` falls to the listable default (canPasteInto: true).
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    m.addToast(...args)
  },
}))

// Zoom arms: getSetting/setSetting back the text-size read/write.
vi.mock('$lib/settings', () => ({
  getSetting: (key: string) => m.getSetting(key),
  setSetting: (...args: unknown[]) => {
    m.setSetting(...args)
  },
}))

// `showZoomToast` reads the reset shortcut. Default: no shortcut bound (menu hint).
vi.mock('$lib/shortcuts', () => ({
  getEffectiveShortcuts: (id: string) => m.getEffectiveShortcuts(id),
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: () => m.openSettingsWindow(),
}))

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: () => {
    m.openErrorReportDialog()
  },
}))

vi.mock('$lib/updates/updater.svelte', () => ({
  runMenuTriggeredCheck: () => m.runMenuTriggeredCheck(),
}))

vi.mock('$lib/downloads/go-to-latest', () => ({
  goToLatestDownload: (...args: unknown[]) => m.goToLatestDownload(...args),
}))

// The whole `$lib/tauri-commands` barrel the arms call.
vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: (...a: unknown[]) => m.openExternalUrl(...a),
  showInFinder: (...a: unknown[]) => m.showInFinder(...a),
  copyToClipboard: (...a: unknown[]) => m.copyToClipboard(...a),
  quickLookOpen: (...a: unknown[]) => m.quickLookOpen(...a),
  quickLookClose: (...a: unknown[]) => m.quickLookClose(...a),
  getInfo: (...a: unknown[]) => m.getInfo(...a),
  openInEditor: (...a: unknown[]) => m.openInEditor(...a),
  syncMenuShowHidden: (...a: unknown[]) => m.syncMenuShowHidden(...a),
  readClipboardText: () => m.readClipboardText(),
  cloudMakeAvailableOffline: (...a: unknown[]) => m.cloudMakeAvailableOffline(...a),
  cloudRemoveDownload: (...a: unknown[]) => m.cloudRemoveDownload(...a),
}))

// QuickLook dispatch guard + the `$state` singleton (reconfigurable per branch).
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({
  quickLookState: m.quickLookState,
  quickLookDispatchGuardJustFired: () => m.quickLookDispatchGuardJustFired(),
  armQuickLookDispatchGuard: () => {
    m.armQuickLookDispatchGuard()
  },
}))

import { handleCommandExecute } from './command-dispatch'
import { DELEGATE_ROWS, makeCtx, makeExplorerSpy } from './command-dispatch.test-harness'

beforeEach(() => {
  vi.clearAllMocks()
  getVolumeId.mockReturnValue('local')
  getPanePath.mockReturnValue('/Users/test')
  getSetting.mockReturnValue(100)
  getEffectiveShortcuts.mockReturnValue([])
  readClipboardText.mockResolvedValue('')
  quickLookDispatchGuardJustFired.mockReturnValue(false)
  quickLookState.isOpen = false
})

// ===========================================================================
// Table-driven: the simple-delegate arms (one method call, exact args).
// Each row pins which explorer method (or dialog callback) fires, with what.
// ===========================================================================
describe('characterization — table-driven simple-delegate arms', () => {
  it.each(DELEGATE_ROWS.map((r) => ({ row: r, label: `${r.id}${r.args ? ' (with args)' : ''}` })))(
    'dispatches $label to its exact call pattern',
    async ({ row }) => {
      const explorer = makeExplorerSpy()
      const ctx = makeCtx(explorer)
      if (row.args === undefined) {
        await handleCommandExecute(row.id, ctx)
      } else {
        // The public generic arg-checks per id; the table holds typed payloads.
        await handleCommandExecute(row.id, ctx, row.args as never)
      }
      row.expect(explorer, ctx.dialogs)
    },
  )
})
