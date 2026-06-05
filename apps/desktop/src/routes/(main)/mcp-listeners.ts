/**
 * MCP event listeners: a thin TRANSPORT ADAPTER between the Tauri events the MCP
 * server emits and the typed command bus (`handleCommandExecute`). Per event, the
 * adapter VALIDATE-parses the raw payload into the command's typed `CommandArgs`
 * (whitelist-checking every discriminant string — a malformed payload collapses to
 * a safe default or a silent skip, never an `as`-cast into a typed arg), then
 * dispatches through the bus. No business logic lives here.
 *
 * Two exceptions stay adapter-local:
 * - **`mcp-nav-to-path`** bypasses the bus entirely. `navigateToPath` returns a
 *   sync `string` refusal sentinel that fire-and-forget `dispatch` can't surface;
 *   the adapter keeps calling `explorerRef.navigateToPath` directly and forwards
 *   the sentinel verbatim (L12). It joins the bus in Phase 3 with `NavigateResult`.
 * - **`mcp-response` round-trips** (`mcp-open-under-cursor`, `mcp-move-cursor`):
 *   the bus dispatches the `void`-returning intent; the adapter owns the
 *   `requestId` correlation and the `emit('mcp-response', …)` reply. It AWAITs the
 *   dispatch so the ack fires only after the action completes (the backend has an
 *   ack timeout).
 *
 * The non-nav `mcp-key` keys keep the `sendKeyToFocusedPane` passthrough — a
 * keystroke is transport, not a command, so it never rides the bus (invariant P2).
 */

import type { ViewMode } from '$lib/app-status-store'
import type {
  CommandDispatchArgs,
  CommandId,
  ConfirmDialogType,
  McpSelectMode,
  McpTabAction,
  SortColumn,
} from '$lib/commands'
import { applySearchPrefill, type SearchPrefill, type SearchMode } from '$lib/search/search-state.svelte'
import type { ExplorerAPI } from './explorer-api'

/**
 * Typed dispatch entry point the adapter calls. Bound by the caller to
 * `handleCommandExecute(id, ctx, ...args)`. Returns the dispatch's `Promise<void>`
 * so the round-trip listeners can await completion before replying.
 */
export type CommandDispatch = <K extends CommandId>(id: K, ...args: CommandDispatchArgs<K>) => Promise<void>

export interface McpListenerContext {
  getExplorer: () => ExplorerAPI | undefined
  /** Typed command-bus dispatch (`handleCommandExecute` bound with its context). */
  dispatch: CommandDispatch
  listenTauri: (event: string, handler: (event: { payload: unknown }) => void) => Promise<void>
  /** Whether AI mode is currently available (provider configured). Drives the default mode. */
  isAiEnabled: () => boolean
}

// === Validating parses ===
// Each returns the typed value for a well-formed input, or `undefined` for a
// malformed one (the listener then skips the dispatch). Pure and unit-tested.

/** A non-empty payload object, or `{}` so optional-field reads stay safe. */
function asRecord(payload: unknown): Record<string, unknown> {
  return payload && typeof payload === 'object' ? (payload as Record<string, unknown>) : {}
}

/** `'left'` / `'right'`, else `undefined`. */
export function parsePane(value: unknown): 'left' | 'right' | undefined {
  return value === 'left' || value === 'right' ? value : undefined
}

/** Sort column. Accepts the MCP `ext` alias for `extension`. */
export function parseSortColumn(value: unknown): SortColumn | undefined {
  if (value === 'ext') return 'extension'
  return value === 'name' || value === 'extension' || value === 'size' || value === 'modified' || value === 'created'
    ? value
    : undefined
}

/** Sort order. The MCP tool never emits `toggle`. */
export function parseSortOrder(value: unknown): 'asc' | 'desc' | undefined {
  return value === 'asc' || value === 'desc' ? value : undefined
}

/** Selection mode. */
export function parseSelectMode(value: unknown): McpSelectMode | undefined {
  return value === 'replace' || value === 'add' || value === 'subtract' ? value : undefined
}

/** Tab action. */
export function parseTabAction(value: unknown): McpTabAction | undefined {
  return value === 'new' ||
    value === 'close' ||
    value === 'close_others' ||
    value === 'activate' ||
    value === 'reopen' ||
    value === 'set_pinned'
    ? value
    : undefined
}

/** View mode. */
export function parseViewMode(value: unknown): ViewMode | undefined {
  return value === 'full' || value === 'brief' ? value : undefined
}

/** Confirm-dialog type. */
export function parseConfirmDialogType(value: unknown): ConfirmDialogType | undefined {
  return value === 'transfer-confirmation' || value === 'delete-confirmation' ? value : undefined
}

/** Selection count: a number, or the `'all'` sentinel. */
export function parseSelectCount(value: unknown): number | 'all' | undefined {
  if (value === 'all') return 'all'
  return typeof value === 'number' ? value : undefined
}

/** Cursor target: a numeric index or a name string. */
export function parseCursorTarget(value: unknown): number | string | undefined {
  return typeof value === 'number' || typeof value === 'string' ? value : undefined
}

// === Typed command ids ===
// `cmdr/no-raw-command-dispatch` (A3) bans literal ids at dispatch sites, so each
// id the adapter dispatches is a typed const. A registry rename then breaks
// compilation here instead of silently no-oping in the switch `default`.
const sortSetCommand: CommandId = 'sort.set'
const volumeSelectByNameCommand: CommandId = 'volume.selectByName'
const selectionMcpSelectCommand: CommandId = 'selection.mcpSelect'
const cursorMoveToCommand: CommandId = 'cursor.moveTo'
const cursorScrollToCommand: CommandId = 'cursor.scrollTo'
const viewSetModeCommand: CommandId = 'view.setMode'
const paneRefreshCommand: CommandId = 'pane.refresh'
const fileCopyCommand: CommandId = 'file.copy'
const fileMoveCommand: CommandId = 'file.move'
const fileNewFolderCommand: CommandId = 'file.newFolder'
const fileNewFileCommand: CommandId = 'file.newFile'
const fileDeleteCommand: CommandId = 'file.delete'
const dialogConfirmCommand: CommandId = 'dialog.confirm'
const navBackCommand: CommandId = 'nav.back'
const navForwardCommand: CommandId = 'nav.forward'
const navOpenUnderCursorCommand: CommandId = 'nav.openUnderCursor'
const searchOpenCommand: CommandId = 'search.open'
const tabMcpActionCommand: CommandId = 'tab.mcpAction'

/** Register all MCP event listeners. Call from onMount after listenTauri is ready. */
export async function setupMcpListeners(ctx: McpListenerContext): Promise<void> {
  const { listenTauri, getExplorer, dispatch, isAiEnabled } = ctx

  await listenTauri('mcp-open-search-dialog', (event) => {
    // The backend strips nulls before emitting, but treat the payload defensively: anything not
    // matching the expected shape collapses to "no prefill", which still opens the dialog with
    // whatever state already lives in the module-level $state.
    const raw = asRecord(event.payload)
    const validModes: SearchMode[] = ['ai', 'filename', 'regex']
    const requestedMode =
      typeof raw.mode === 'string' && validModes.includes(raw.mode as SearchMode) ? (raw.mode as SearchMode) : undefined
    // Default mode per plan §3.11 tool docs: 'ai' if AI on, else 'filename'.
    const defaultedMode: SearchMode = requestedMode ?? (isAiEnabled() ? 'ai' : 'filename')

    const prefill: SearchPrefill = {
      query: typeof raw.query === 'string' ? raw.query : undefined,
      mode: defaultedMode,
      sizeMin: typeof raw.sizeMin === 'number' ? raw.sizeMin : undefined,
      sizeMax: typeof raw.sizeMax === 'number' ? raw.sizeMax : undefined,
      modifiedAfter: typeof raw.modifiedAfter === 'string' ? raw.modifiedAfter : undefined,
      modifiedBefore: typeof raw.modifiedBefore === 'string' ? raw.modifiedBefore : undefined,
      scope: typeof raw.scope === 'string' ? raw.scope : undefined,
      caseSensitive: typeof raw.caseSensitive === 'boolean' ? raw.caseSensitive : undefined,
      excludeSystemDirs: typeof raw.excludeSystemDirs === 'boolean' ? raw.excludeSystemDirs : undefined,
      autoRun: typeof raw.autoRun === 'boolean' ? raw.autoRun : undefined,
    }
    // Prefill is adapter-local state (module-level $state); opening the dialog is
    // the `search.open` command. Apply the prefill FIRST so the dialog reads it on
    // open, then dispatch — the dispatch flips `showSearchDialog` synchronously
    // (before any `await`), so the order holds.
    applySearchPrefill(prefill)
    void dispatch(searchOpenCommand)
  })

  await listenTauri('mcp-key', (event) => {
    const raw = asRecord(event.payload)
    const key = typeof raw.key === 'string' ? raw.key : undefined
    if (key === undefined) return
    if (key === 'GoBack') {
      // Routes through the bus but keeps calling the OLD `navigate` entry (Phase-2
      // sequencing rule — nav mechanism retires in Phase 3).
      void dispatch(navBackCommand)
    } else if (key === 'GoForward') {
      void dispatch(navForwardCommand)
    } else {
      // P2: a keystroke is transport, not a command — it never rides the bus.
      getExplorer()?.sendKeyToFocusedPane(key)
    }
  })

  await listenTauri('mcp-sort', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const column = parseSortColumn(raw.by)
    const order = parseSortOrder(raw.order)
    if (!pane || !column || !order) return
    void dispatch(sortSetCommand, { pane, column, order })
  })

  await listenTauri('mcp-volume-select', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const name = typeof raw.name === 'string' ? raw.name : undefined
    if (!pane || name === undefined) return
    void dispatch(volumeSelectByNameCommand, { pane, name })
  })

  await listenTauri('mcp-select', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const start = typeof raw.start === 'number' ? raw.start : undefined
    const count = parseSelectCount(raw.count)
    const mode = parseSelectMode(raw.mode)
    if (!pane || start === undefined || count === undefined || !mode) return
    void dispatch(selectionMcpSelectCommand, { pane, start, count, mode })
  })

  await listenTauri('mcp-nav-to-path', (event) => {
    // STAYS OFF THE BUS (Phase 2). The sync-refusal `string` sentinel can't pass
    // through fire-and-forget `dispatch`; the adapter calls `navigateToPath`
    // directly and forwards the sentinel byte-identically (L12). Joins the bus in
    // Phase 3 with `NavigateResult`.
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const path = typeof raw.path === 'string' ? raw.path : undefined
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (!pane || path === undefined) return
    const explorerRef = getExplorer()
    // explorerRef may be null during HMR; skip silently, let the backend timeout handle it
    if (!explorerRef) return
    const result = explorerRef.navigateToPath(pane, path)
    if (requestId) {
      void (async () => {
        const { emit } = await import('@tauri-apps/api/event')
        if (typeof result === 'string') {
          // Synchronous error (pane not available, wrong volume, etc.)
          await emit('mcp-response', { requestId, ok: false, error: result })
        } else {
          // Promise: wait for directory listing to complete
          try {
            await result
            await emit('mcp-response', { requestId, ok: true })
          } catch (e) {
            const error = e instanceof Error ? e.message : String(e)
            await emit('mcp-response', { requestId, ok: false, error })
          }
        }
      })()
    }
  })

  // Round-trip for open-under-cursor: backend can't infer outcome from state pushes
  // alone (Enter on a non-directory file delegates to the OS default app and produces
  // no MCP-observable signal). The bus dispatches the intent; the adapter awaits the
  // dispatch's promise (which awaits `openItemUnderCursor`) and replies via mcp-response.
  await listenTauri('mcp-open-under-cursor', (event) => {
    const raw = asRecord(event.payload)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      // HMR can land this with no explorer; reply ok:false rather than crashing.
      if (!getExplorer()) {
        await emit('mcp-response', { requestId, ok: false, error: 'Explorer is not ready' })
        return
      }
      try {
        await dispatch(navOpenUnderCursorCommand)
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-move-cursor', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const to = parseCursorTarget(raw.to)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      if (!pane || to === undefined) {
        await emit('mcp-response', { requestId, ok: false, error: 'Invalid move-cursor payload' })
        return
      }
      try {
        // AWAIT the dispatch so the ack fires only after `moveCursor` settles. L1/L2
        // (focus re-anchor + `whenLoadSettles`) live inside `moveCursor` — untouched.
        await dispatch(cursorMoveToCommand, { pane, to })
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-scroll-to', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const index = typeof raw.index === 'number' ? raw.index : undefined
    if (!pane || index === undefined) return
    void dispatch(cursorScrollToCommand, { pane, index })
  })

  await listenTauri('mcp-set-view-mode', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const mode = parseViewMode(raw.mode)
    if (!pane || !mode) return
    // `fromMenu: false` → the handler pushes the menu state (nothing toggled it),
    // matching the old `setViewMode(mode, pane)` path byte-for-byte.
    void dispatch(viewSetModeCommand, { pane, mode, fromMenu: false })
  })

  await listenTauri('mcp-refresh', () => {
    void dispatch(paneRefreshCommand)
  })

  await listenTauri('mcp-copy', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    const onConflict = typeof raw.onConflict === 'string' ? raw.onConflict : undefined
    void dispatch(fileCopyCommand, { autoConfirm, onConflict })
  })

  await listenTauri('mcp-move', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    const onConflict = typeof raw.onConflict === 'string' ? raw.onConflict : undefined
    void dispatch(fileMoveCommand, { autoConfirm, onConflict })
  })

  await listenTauri('mcp-mkdir', () => {
    void dispatch(fileNewFolderCommand)
  })

  await listenTauri('mcp-mkfile', () => {
    void dispatch(fileNewFileCommand)
  })

  await listenTauri('mcp-delete', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    void dispatch(fileDeleteCommand, { autoConfirm })
  })

  await listenTauri('mcp-confirm-dialog', (event) => {
    const raw = asRecord(event.payload)
    const type = parseConfirmDialogType(raw.type)
    const onConflict = typeof raw.onConflict === 'string' ? raw.onConflict : undefined
    if (!type) return
    void dispatch(dialogConfirmCommand, { type, onConflict })
  })

  await listenTauri('mcp-tab', (event) => {
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const action = parseTabAction(raw.action)
    if (!pane || !action) return
    const tabId = typeof raw.tabId === 'string' ? raw.tabId : undefined
    const pinned = typeof raw.pinned === 'boolean' ? raw.pinned : undefined
    void dispatch(tabMcpActionCommand, { pane, action, tabId, pinned })
  })
}
