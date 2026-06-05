/**
 * View handlers: hidden-file toggle, brief/full mode, the per-pane `view.setMode`,
 * and the six zoom ids (the four presets share one body; in/out clamp). The
 * `showZoomToast` helper is view-only, so it lives here.
 */
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getSetting, setSetting } from '$lib/settings'
import { syncMenuShowHidden } from '$lib/tauri-commands'
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

/**
 * Shows a transient toast confirming a zoom change. Surfaces the reset shortcut
 * (or menu path if no shortcut is bound) so users who hit ⌘+/⌘- by accident
 * know how to get back to 100%.
 */
function showZoomToast(oldSize: number, newSize: number): void {
  if (oldSize === newSize) return

  const resetShortcut = getEffectiveShortcuts('view.zoom.set100')[0]
  const resetHint = resetShortcut
    ? `You can reset the zoom level to 100% by ${resetShortcut}.`
    : 'You can reset the zoom level to 100% at View > Zoom > 100%.'

  let message: string
  if (newSize === 100) {
    message = 'Zoom reset to 100%.'
  } else if (newSize > oldSize) {
    message = `Zoom increased to ${String(newSize)}%. ${resetHint}`
  } else {
    message = `Zoom decreased to ${String(newSize)}%. ${resetHint}`
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
  'view.showHidden': ({ explorerRef }) => {
    // Local-first toggle: flip FE state synchronously so the listing
    // re-fetch effects land in the next Svelte tick, then push the new
    // check state to the native menu fire-and-forget. The previous
    // implementation routed through `toggle_hidden_files` (Rust toggle +
    // `settings-changed` emit + FE listener), which added an IPC + event
    // hop and caused the `toggles hidden file visibility` e2e test to
    // flake ~1/25 runs under slow-lane load.
    if (!explorerRef) return
    const newState = explorerRef.toggleHiddenFiles()
    void syncMenuShowHidden(newState)
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
