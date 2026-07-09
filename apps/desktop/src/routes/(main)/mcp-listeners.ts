/**
 * MCP event listeners: a thin TRANSPORT ADAPTER between the Tauri events the MCP
 * server emits and the typed command bus (`handleCommandExecute`). Per event, the
 * adapter VALIDATE-parses the raw payload into the command's typed `CommandArgs`
 * (whitelist-checking every discriminant string — a malformed payload collapses to
 * a safe default or a silent skip, never an `as`-cast into a typed arg), then
 * dispatches through the bus. No business logic lives here.
 *
 * Two exceptions stay adapter-local:
 * - **`mcp-nav-to-path`** bypasses the bus entirely. `navigate()` returns a typed
 *   `NavigateResult` value that fire-and-forget `dispatch` can't surface; the
 *   adapter calls `explorerRef.navigate({ … })` directly and forwards the refusal
 *   `message` verbatim as the `mcp-response` error (L12 — byte-identical to the old
 *   sync `string` sentinel).
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
import { resolveLocation } from '$lib/file-explorer/navigation/resolve-location'
import { tString } from '$lib/intl/messages.svelte'
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

/** Names list for by-name selection: a non-empty array of strings. */
export function parseNames(value: unknown): string[] | undefined {
  if (!Array.isArray(value) || value.length === 0) return undefined
  return value.every((v): v is string => typeof v === 'string') ? value : undefined
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
const selectionMcpSelectByNamesCommand: CommandId = 'selection.mcpSelectByNames'
const cursorMoveToCommand: CommandId = 'cursor.moveTo'
const cursorScrollToCommand: CommandId = 'cursor.scrollTo'
const viewSetModeCommand: CommandId = 'view.setMode'
const paneRefreshCommand: CommandId = 'pane.refresh'
const fileCopyCommand: CommandId = 'file.copy'
const fileMoveCommand: CommandId = 'file.move'
const fileCompressCommand: CommandId = 'file.compress'
const fileNewFolderCommand: CommandId = 'file.newFolder'
const fileNewFileCommand: CommandId = 'file.newFile'
const fileDeleteCommand: CommandId = 'file.delete'
const fileRenameCommand: CommandId = 'file.rename'
const dialogConfirmCommand: CommandId = 'dialog.confirm'
const navBackCommand: CommandId = 'nav.back'
const navForwardCommand: CommandId = 'nav.forward'
const navOpenUnderCursorCommand: CommandId = 'nav.openUnderCursor'
const searchOpenCommand: CommandId = 'search.open'
const tabMcpActionCommand: CommandId = 'tab.mcpAction'

/**
 * Runs an MCP direct-create (mkdir/mkfile autoConfirm) and replies on the
 * `requestId` round-trip: ok on success, or the backend conflict / read-only
 * refusal error. `create` returns `undefined` when the explorer isn't ready
 * (HMR / teardown), which we surface as a clean failure rather than a false ok.
 */
async function createDirectlyThenReply(
  create: () => Promise<void> | undefined,
  name: string | undefined,
  requestId: string | undefined,
): Promise<void> {
  if (requestId === undefined) return
  const { emit } = await import('@tauri-apps/api/event')
  if (name === undefined) {
    await emit('mcp-response', { requestId, ok: false, error: 'autoConfirm requires a name' })
    return
  }
  const pending = create()
  if (pending === undefined) {
    await emit('mcp-response', { requestId, ok: false, error: 'Explorer is not ready' })
    return
  }
  try {
    await pending
    await emit('mcp-response', { requestId, ok: true })
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e)
    await emit('mcp-response', { requestId, ok: false, error })
  }
}

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
      isDirectory: typeof raw.isDirectory === 'boolean' ? raw.isDirectory : undefined,
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
      // Routes through the bus; the `nav.back` handler drives the `navigate()`
      // transaction (a keystroke is transport, the resolved command is not).
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
    // Round-trip: the adapter replies after the selection landed in the backend's
    // PaneStateStore, so a follow-up tool call (select → copy) reads fresh state.
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const start = typeof raw.start === 'number' ? raw.start : undefined
    const count = parseSelectCount(raw.count)
    const mode = parseSelectMode(raw.mode)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      if (!pane || start === undefined || count === undefined || !mode) {
        await emit('mcp-response', { requestId, ok: false, error: 'Invalid select payload' })
        return
      }
      try {
        await dispatch(selectionMcpSelectCommand, { pane, start, count, mode })
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-select-names', (event) => {
    // Round-trip (like `mcp-move-cursor`): by-name selection must report missing
    // names back, so the adapter owns the requestId correlation and AWAITs the
    // dispatch — a not-found throw becomes the `mcp-response` error.
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const names = parseNames(raw.names)
    const mode = parseSelectMode(raw.mode)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      if (!pane || !names || !mode) {
        await emit('mcp-response', { requestId, ok: false, error: 'Invalid select-names payload' })
        return
      }
      try {
        await dispatch(selectionMcpSelectByNamesCommand, { pane, names, mode })
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-nav-to-path', (event) => {
    // STAYS OFF THE BUS (master § Bus interplay). `navigate()` returns a typed
    // `NavigateResult` value the fire-and-forget `dispatch` can't surface, so the
    // adapter calls `navigate()` directly and forwards the refusal `message`
    // verbatim as the `mcp-response` error (L12 — refusal strings byte-identical
    // to the old `typeof result === 'string'` branch). The `requestId` round-trip
    // + `emit` stay adapter-local.
    //
    // The bare path resolves to a `Location` at the edge first: the agent path can
    // live on ANY volume, so an unresolvable path (drive gone) is an honest typed
    // refusal, not a wrong-volume listing. This also NARROWS the on-network
    // refusal: a LOCAL target from a network pane now resolves to `root` ≠ the
    // network volume → the switch arm switches and navigates; only an `smb://`
    // target (which `resolve_location` maps back to the virtual `network` id)
    // still hits the in-place on-network refusal.
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const path = typeof raw.path === 'string' ? raw.path : undefined
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (!pane || path === undefined) return
    const explorerRef = getExplorer()
    // explorerRef may be null during HMR; skip silently, let the backend timeout handle it
    if (!explorerRef) return
    void (async () => {
      const reply = async (body: { ok: true } | { ok: false; error: string }): Promise<void> => {
        if (requestId === undefined) return
        const { emit } = await import('@tauri-apps/api/event')
        await emit('mcp-response', { requestId, ...body })
      }

      const outcome = await resolveLocation(path)
      if (!outcome.ok) {
        await reply({ ok: false, error: tString('fileExplorer.navigation.locationUnreachableToast') })
        return
      }
      const result = explorerRef.navigate({ pane, to: { goTo: outcome.location }, source: 'mcp' })
      if (result.status === 'refused') {
        // Synchronous refusal (pane not available, on the network volume for an
        // smb:// target, etc.) — forward the exact refusal string the agent reads.
        await reply({ ok: false, error: result.reason.message })
        return
      }
      // Started: wait for the navigation to settle (the listing completes).
      try {
        await result.settled
        await reply({ ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await reply({ ok: false, error })
      }
    })()
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

  await listenTauri('mcp-refresh', (event) => {
    // Round-trip: the refresh tool's OK must mean "the backend re-read the
    // directory", not "an event was dispatched" — a stale cache is exactly what
    // the caller is trying to escape.
    const raw = asRecord(event.payload)
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      try {
        await dispatch(paneRefreshCommand)
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-copy', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    const onConflict = typeof raw.onConflict === 'string' ? raw.onConflict : undefined
    // Auto-confirm carries a round-trip id: the FE replies `mcp-response` with the
    // spawned operationId once the op starts (see transfer-progress-state).
    const mcpRequestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    void dispatch(fileCopyCommand, { autoConfirm, onConflict, mcpRequestId })
  })

  await listenTauri('mcp-move', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    const onConflict = typeof raw.onConflict === 'string' ? raw.onConflict : undefined
    const mcpRequestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    void dispatch(fileMoveCommand, { autoConfirm, onConflict, mcpRequestId })
  })

  await listenTauri('mcp-compress', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    // No onConflict, unlike copy/move: compress has no inner-file conflicts, and an
    // existing target archive is the dialog's overwrite affordance, not a policy.
    const mcpRequestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    void dispatch(fileCompressCommand, { autoConfirm, mcpRequestId })
  })

  await listenTauri('mcp-rename', (event) => {
    // Round-trip: the non-autoConfirm `rename` tool starts the inline editor
    // prefilled with `newName` for the user to review. Resolution lives here (the
    // FE holds the live listing): when `name` is given we move the cursor to that
    // row first (which errors honestly if it isn't in the listing and pins
    // activation via `expectedName`); without a name we rename the cursor item.
    const raw = asRecord(event.payload)
    const pane = parsePane(raw.pane)
    const name = typeof raw.name === 'string' ? raw.name : undefined
    const newName = typeof raw.newName === 'string' ? raw.newName : undefined
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (requestId === undefined) return
    void (async () => {
      const { emit } = await import('@tauri-apps/api/event')
      if (!pane || newName === undefined) {
        await emit('mcp-response', { requestId, ok: false, error: 'Invalid rename payload' })
        return
      }
      try {
        if (name !== undefined) {
          await dispatch(cursorMoveToCommand, { pane, to: name })
        }
        await dispatch(fileRenameCommand, { initialName: newName, expectedName: name })
        await emit('mcp-response', { requestId, ok: true })
      } catch (e) {
        const error = e instanceof Error ? e.message : String(e)
        await emit('mcp-response', { requestId, ok: false, error })
      }
    })()
  })

  await listenTauri('mcp-mkdir', (event) => {
    // `name` prefills the naming dialog; with `autoConfirm` it's a round-trip that
    // creates directly on the pane's LIVE path (so it can't land in a stale dir)
    // and replies OK or the backend conflict error.
    const raw = asRecord(event.payload)
    const name = typeof raw.name === 'string' ? raw.name : undefined
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (raw.autoConfirm === true) {
      void createDirectlyThenReply(() => getExplorer()?.createFolderDirect(name ?? ''), name, requestId)
    } else {
      void dispatch(fileNewFolderCommand, { name })
    }
  })

  await listenTauri('mcp-mkfile', (event) => {
    const raw = asRecord(event.payload)
    const name = typeof raw.name === 'string' ? raw.name : undefined
    const requestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    if (raw.autoConfirm === true) {
      void createDirectlyThenReply(() => getExplorer()?.createFileDirect(name ?? ''), name, requestId)
    } else {
      void dispatch(fileNewFileCommand, { name })
    }
  })

  await listenTauri('mcp-delete', (event) => {
    const raw = asRecord(event.payload)
    const autoConfirm = typeof raw.autoConfirm === 'boolean' ? raw.autoConfirm : undefined
    // `permanent` rides the event only when the tool's `mode` was given; otherwise
    // the FE applies its per-volume default (trash where supported).
    const permanent = typeof raw.permanent === 'boolean' ? raw.permanent : undefined
    const mcpRequestId = typeof raw.requestId === 'string' ? raw.requestId : undefined
    void dispatch(fileDeleteCommand, { autoConfirm, permanent, mcpRequestId })
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
