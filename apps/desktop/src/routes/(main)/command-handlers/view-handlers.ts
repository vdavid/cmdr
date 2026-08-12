/**
 * View handlers: hidden-file toggle, brief/full mode, the per-pane `view.setMode`,
 * and the six zoom ids (the four presets share one body; in/out clamp). The
 * `showZoomToast` helper is view-only, so it lives here.
 */
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts, toDisplayShortcut } from '$lib/shortcuts'
import { getSetting, setSetting } from '$lib/settings'
import { tString } from '$lib/intl/messages.svelte'
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

/**
 * Shows a transient toast confirming a zoom change. Surfaces the reset shortcut
 * (or menu path if no shortcut is bound) so users who hit ⌘+/⌘- by accident
 * know how to get back to 100%.
 */
function showZoomToast(oldSize: number, newSize: number): void {
  if (oldSize === newSize) return

  const resetShortcut = toDisplayShortcut(getEffectiveShortcuts('view.zoom.set100')[0] ?? '')
  const resetHint = resetShortcut
    ? tString('commands.handler.zoomResetHintShortcut', { shortcut: resetShortcut })
    : tString('commands.handler.zoomResetHintMenu')

  let message: string
  if (newSize === 100) {
    message = tString('commands.handler.zoomReset')
  } else if (newSize > oldSize) {
    message = tString('commands.handler.zoomIncreased', { size: String(newSize), hint: resetHint })
  } else {
    message = tString('commands.handler.zoomDecreased', { size: String(newSize), hint: resetHint })
  }

  addToast(message, { level: 'info', id: 'zoom-change' })
}

/** Shared body for the four `view.zoom.setNN` presets (the arg differs by id). */
function applyZoomPreset(preset: number): void {
  const current = getSetting('appearance.textSize')
  setSetting('appearance.textSize', preset)
  showZoomToast(current, preset)
}

export const viewHandlers = {
  'view.showHidden': () => {
    // Local-first toggle: `setSetting` writes its in-memory cache synchronously,
    // so both panes' listing re-fetch effects land in the next Svelte tick. The
    // native menu's check state follows from `settings-applier.ts`, and the save
    // is debounced behind it. ❌ Don't route this through Rust's
    // `toggle_hidden_files` instead: that IPC → event → effect → DOM chain is
    // what flaked the `toggles hidden file visibility` e2e test ~1/25 runs.
    setSetting('listing.showHiddenFiles', !getSetting('listing.showHiddenFiles'))
  },

  'view.briefMode': ({ explorerRef }) => {
    explorerRef?.setViewMode('brief')
  },

  'view.fullMode': ({ explorerRef }) => {
    explorerRef?.setViewMode('full')
  },

  'view.setMode': ({ explorerRef, dispatchArgs }) => {
    // Per-pane view change. The `id === 'view.setMode'` narrowing doesn't reach
    // `dispatchArgs` (it's a separate local), so read the typed payload with a
    // single cast — the generic signature already type-checked it at the call
    // site. `fromMenu` picks the primitive: a native-menu click
    // (`view-mode-changed`, `fromMenu: true`) routes to `setViewModeFromMenu`,
    // which skips `pushViewMenuState` because the click already toggled its own
    // CheckMenuItem (Rust ran `sync_view_mode_check_states`); the MCP
    // `set_view_mode` tool (`fromMenu: false`) routes to `setViewMode`, which
    // pushes the menu state since nothing toggled it.
    const { pane, mode, fromMenu } = dispatchArgs as CommandArgs['view.setMode']
    if (fromMenu) explorerRef?.setViewModeFromMenu(pane, mode)
    else explorerRef?.setViewMode(mode, pane)
  },

  // === Zoom commands ===
  // Each writes `appearance.textSize`; the settings store cross-window-syncs
  // and `lib/text-size.svelte.ts` recomputes the effective scale.
  'view.zoom.set75': () => {
    applyZoomPreset(75)
  },
  'view.zoom.set100': () => {
    applyZoomPreset(100)
  },
  'view.zoom.set125': () => {
    applyZoomPreset(125)
  },
  'view.zoom.set150': () => {
    applyZoomPreset(150)
  },

  'view.zoom.in': () => {
    const current = getSetting('appearance.textSize')
    const next = Math.min(150, current + 10)
    setSetting('appearance.textSize', next)
    showZoomToast(current, next)
  },

  'view.zoom.out': () => {
    const current = getSetting('appearance.textSize')
    const next = Math.max(75, current - 10)
    setSetting('appearance.textSize', next)
    showZoomToast(current, next)
  },
} satisfies Partial<CommandHandlerRecord>
