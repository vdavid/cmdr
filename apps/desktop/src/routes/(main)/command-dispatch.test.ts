/**
 * Unit coverage for the arg-carrying dispatch shape.
 *
 * `view.setMode` is the first command that carries typed args (`{ pane, mode }`).
 * This pins that `handleCommandExecute('view.setMode', ctx, args)` reaches the
 * right per-pane primitive (`setViewModeFromMenu`) with the args intact — the
 * native-menu `view-mode-changed` event's path onto the bus. The routes file has
 * no coverage gate, so this is a behavioral guard, not a coverage filler.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// `getAppLogger('user-action')` runs at module top-level in command-dispatch,
// and the dispatch preamble fires `record_breadcrumb` via `invoke`. Mock both so
// the import is side-effect-free and the breadcrumb IPC is a no-op.
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }))
// The search-results guard reads the focused pane's volume id; keep it off the
// blocked virtual pane so dispatch proceeds to the handler.
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: () => 'local',
  getFocusedPanePath: () => '/Users/test',
}))

import { handleCommandExecute, type CommandDispatchContext } from './command-dispatch'
import type { ExplorerAPI } from './explorer-api'

function makeCtx(explorer: Partial<ExplorerAPI>): CommandDispatchContext {
  return {
    getExplorer: () => explorer as ExplorerAPI,
    dialogs: {
      showCommandPalette: vi.fn(),
      showSearchDialog: vi.fn(),
      showGoToPathDialog: vi.fn(),
      showAboutWindow: vi.fn(),
      showLicenseKeyDialog: vi.fn(),
      showSelectionDialog: vi.fn(),
      openOnboarding: vi.fn(),
    },
  }
}

describe('handleCommandExecute — view.setMode (arg-carrying dispatch)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('fromMenu: true routes to setViewModeFromMenu, not setViewMode', async () => {
    const setViewModeFromMenu = vi.fn()
    const setViewMode = vi.fn()
    const ctx = makeCtx({ setViewModeFromMenu, setViewMode })

    await handleCommandExecute('view.setMode', ctx, { pane: 'right', mode: 'brief', fromMenu: true })

    expect(setViewModeFromMenu).toHaveBeenCalledExactlyOnceWith('right', 'brief')
    // The menu already toggled its CheckMenuItem, so the focused-pane setter
    // (which would push menu state) must NOT run.
    expect(setViewMode).not.toHaveBeenCalled()
  })

  it('fromMenu: false routes to setViewMode (the MCP path that pushes menu state)', async () => {
    const setViewModeFromMenu = vi.fn()
    const setViewMode = vi.fn()
    const ctx = makeCtx({ setViewModeFromMenu, setViewMode })

    await handleCommandExecute('view.setMode', ctx, { pane: 'left', mode: 'full', fromMenu: false })

    // `setViewMode(mode, pane)` — note arg order — pushes the menu state since
    // nothing toggled it (the MCP `set_view_mode` tool's byte-identical path).
    expect(setViewMode).toHaveBeenCalledExactlyOnceWith('full', 'left')
    expect(setViewModeFromMenu).not.toHaveBeenCalled()
  })
})
