/**
 * Unit coverage for the MCP adapter's validating parsers, plus the `mcp-refresh`
 * round-trip listener.
 *
 * The adapter never `as`-casts a raw event payload into a typed `CommandArgs`: it
 * whitelist-parses every discriminant string, and a malformed value collapses to
 * `undefined` so the listener skips the dispatch (a malformed payload must not
 * reach a handler). These pure parsers carry that contract; the listener wiring
 * itself (a routes module) has no coverage gate, so this pins the load-bearing
 * parts — the parsers, and the refresh round-trip's reply discipline (ack only
 * after the dispatch settles; failures forwarded; no reply without a requestId).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import {
  parsePane,
  parseSortColumn,
  parseSortOrder,
  parseSelectMode,
  parseTabAction,
  parseViewMode,
  parseConfirmDialogType,
  parseSelectCount,
  parseCursorTarget,
  setupMcpListeners,
  type CommandDispatch,
} from './mcp-listeners'
import type { ExplorerAPI } from './explorer-api'
import type { NavigateResult } from '$lib/file-explorer/pane/navigate'

const { resolveLocationMock } = vi.hoisted(() => ({ resolveLocationMock: vi.fn() }))
vi.mock('$lib/file-explorer/navigation/resolve-location', () => ({ resolveLocation: resolveLocationMock }))

describe('parsePane', () => {
  it('accepts left/right', () => {
    expect(parsePane('left')).toBe('left')
    expect(parsePane('right')).toBe('right')
  })
  it('rejects anything else', () => {
    for (const bad of ['both', '', 'Left', 0, null, undefined, {}]) {
      expect(parsePane(bad)).toBeUndefined()
    }
  })
})

describe('parseSortColumn', () => {
  it('accepts the canonical columns', () => {
    for (const col of ['name', 'extension', 'size', 'modified', 'created'] as const) {
      expect(parseSortColumn(col)).toBe(col)
    }
  })
  it('maps the MCP `ext` alias to `extension`', () => {
    expect(parseSortColumn('ext')).toBe('extension')
  })
  it('rejects unknown columns', () => {
    for (const bad of ['date', '', 'NAME', 42, null]) {
      expect(parseSortColumn(bad)).toBeUndefined()
    }
  })
})

describe('parseSortOrder', () => {
  it('accepts asc/desc', () => {
    expect(parseSortOrder('asc')).toBe('asc')
    expect(parseSortOrder('desc')).toBe('desc')
  })
  it('rejects toggle and unknowns (the MCP tool never emits toggle)', () => {
    for (const bad of ['toggle', 'ascending', '', null]) {
      expect(parseSortOrder(bad)).toBeUndefined()
    }
  })
})

describe('parseSelectMode', () => {
  it('accepts replace/add/subtract', () => {
    for (const mode of ['replace', 'add', 'subtract'] as const) {
      expect(parseSelectMode(mode)).toBe(mode)
    }
  })
  it('rejects unknowns', () => {
    for (const bad of ['remove', '', 'Add', null]) {
      expect(parseSelectMode(bad)).toBeUndefined()
    }
  })
})

describe('parseTabAction', () => {
  it('accepts every tab action', () => {
    for (const action of ['new', 'close', 'close_others', 'activate', 'reopen', 'set_pinned'] as const) {
      expect(parseTabAction(action)).toBe(action)
    }
  })
  it('rejects unknowns', () => {
    for (const bad of ['open', 'closeOthers', '', null]) {
      expect(parseTabAction(bad)).toBeUndefined()
    }
  })
})

describe('parseViewMode', () => {
  it('accepts full/brief', () => {
    expect(parseViewMode('full')).toBe('full')
    expect(parseViewMode('brief')).toBe('brief')
  })
  it('rejects unknowns', () => {
    for (const bad of ['list', '', 'Full', null]) {
      expect(parseViewMode(bad)).toBeUndefined()
    }
  })
})

describe('parseConfirmDialogType', () => {
  it('accepts the two dialog kinds', () => {
    expect(parseConfirmDialogType('transfer-confirmation')).toBe('transfer-confirmation')
    expect(parseConfirmDialogType('delete-confirmation')).toBe('delete-confirmation')
  })
  it('rejects unknowns (including the bare `transfer` short form)', () => {
    for (const bad of ['transfer', 'delete', '', null]) {
      expect(parseConfirmDialogType(bad)).toBeUndefined()
    }
  })
})

describe('parseSelectCount', () => {
  it('accepts the `all` sentinel and any number (including 0)', () => {
    expect(parseSelectCount('all')).toBe('all')
    expect(parseSelectCount(0)).toBe(0)
    expect(parseSelectCount(7)).toBe(7)
  })
  it('rejects non-number, non-`all` values', () => {
    for (const bad of ['7', '', null, undefined, {}]) {
      expect(parseSelectCount(bad)).toBeUndefined()
    }
  })
})

describe('parseCursorTarget', () => {
  it('accepts a numeric index or a name string', () => {
    expect(parseCursorTarget(3)).toBe(3)
    expect(parseCursorTarget('README.md')).toBe('README.md')
    expect(parseCursorTarget('')).toBe('')
  })
  it('rejects other types', () => {
    for (const bad of [null, undefined, {}, true]) {
      expect(parseCursorTarget(bad)).toBeUndefined()
    }
  })
})

// === mcp-refresh round-trip ===
// The MCP `refresh` tool's ack is an explicit per-request reply (`mcp-response`
// with the request's id), NOT a pane-state push — so it works even when the
// re-listing is byte-identical to the cached state and the state-push dedupe
// swallows the push. These tests pin the reply discipline.

type TauriEventHandler = (event: { payload: unknown }) => void

async function setupWithHandlers(dispatch: CommandDispatch): Promise<Map<string, TauriEventHandler>> {
  const handlers = new Map<string, TauriEventHandler>()
  await setupMcpListeners({
    getExplorer: () => undefined,
    dispatch,
    listenTauri: (event, handler) => {
      handlers.set(event, handler)
      return Promise.resolve()
    },
    isAiEnabled: () => false,
  })
  return handlers
}

function getHandler(handlers: Map<string, TauriEventHandler>, name: string): TauriEventHandler {
  const handler = handlers.get(name)
  if (!handler) throw new Error(`No listener registered for ${name}`)
  return handler
}

/** Let the listener's async IIFE (dynamic import + dispatch + emit) settle. */
async function flushAsyncWork(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0))
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('mcp-refresh listener (round-trip)', () => {
  beforeEach(() => {
    vi.mocked(emit).mockClear()
  })

  it('replies ok on the request id only after the pane.refresh dispatch resolves', async () => {
    let resolveDispatch!: () => void
    const dispatch = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveDispatch = resolve
        }),
    ) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-refresh')({ payload: { requestId: 'req-7' } })
    await flushAsyncWork()

    // The dispatch is still in flight: no reply yet. The ack must mean "the
    // re-listing settled", not "the event arrived".
    expect(dispatch).toHaveBeenCalledExactlyOnceWith('pane.refresh')
    expect(emit).not.toHaveBeenCalled()

    resolveDispatch()
    await flushAsyncWork()
    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', { requestId: 'req-7', ok: true })
  })

  it('forwards a dispatch failure as the mcp-response error', async () => {
    const dispatch = vi.fn(() =>
      Promise.reject(new Error('Refresh timed out — the volume may be unresponsive')),
    ) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-refresh')({ payload: { requestId: 'req-8' } })
    await flushAsyncWork()

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-8',
      ok: false,
      error: 'Refresh timed out — the volume may be unresponsive',
    })
  })

  it('skips silently without a requestId (the backend round-trip owns the timeout)', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-refresh')({ payload: {} })
    await flushAsyncWork()

    expect(dispatch).not.toHaveBeenCalled()
    expect(emit).not.toHaveBeenCalled()
  })
})

// === MCP-originated write provenance ===
// Every write an MCP tool triggers must carry `initiator: 'aiClient'` through the
// bus so the backend's operation log records the AI as the initiator, not the
// user. These pin the tag on each of the six write dispatches (copy, move,
// compress, mkdir, mkfile, delete) alongside the existing autoConfirm/onConflict
// pass-through.

describe('MCP write dispatches tag initiator: aiClient', () => {
  it('mcp-copy forwards initiator alongside autoConfirm/onConflict', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-copy')({ payload: { autoConfirm: true, onConflict: 'overwrite_all' } })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.copy', {
      autoConfirm: true,
      onConflict: 'overwrite_all',
      initiator: 'aiClient',
    })
  })

  it('mcp-move forwards initiator alongside autoConfirm/onConflict', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-move')({ payload: { autoConfirm: false, onConflict: 'skip_all' } })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.move', {
      autoConfirm: false,
      onConflict: 'skip_all',
      initiator: 'aiClient',
    })
  })

  it('mcp-compress forwards initiator (no onConflict for compress)', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-compress')({ payload: { autoConfirm: true } })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.compress', {
      autoConfirm: true,
      initiator: 'aiClient',
    })
  })

  it('mcp-mkdir dispatches with initiator even though it carries no other args', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-mkdir')({ payload: {} })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.newFolder', { initiator: 'aiClient' })
  })

  it('mcp-mkfile dispatches with initiator even though it carries no other args', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-mkfile')({ payload: {} })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.newFile', { initiator: 'aiClient' })
  })

  it('mcp-delete forwards initiator alongside autoConfirm', async () => {
    const dispatch = vi.fn(() => Promise.resolve()) as unknown as CommandDispatch
    const handlers = await setupWithHandlers(dispatch)

    getHandler(handlers, 'mcp-delete')({ payload: { autoConfirm: true } })

    expect(dispatch).toHaveBeenCalledExactlyOnceWith('file.delete', {
      autoConfirm: true,
      initiator: 'aiClient',
    })
  })
})

// === mcp-nav-to-path round-trip ===
// `nav_to_path` resolves the bare path to a `Location` at the edge first (the
// agent path can live on any volume), then routes a `{ location }` navigation.
// An unresolvable path is an honest `ok: false`, not a wrong-volume listing; a
// synchronous navigate refusal forwards its exact message verbatim (L12).

describe('mcp-nav-to-path listener', () => {
  beforeEach(() => {
    vi.mocked(emit).mockClear()
    resolveLocationMock.mockReset()
  })

  const setFocusedPaneMock = vi.fn()

  async function setupWithExplorer(navigate: () => NavigateResult): Promise<Map<string, TauriEventHandler>> {
    setFocusedPaneMock.mockClear()
    const handlers = new Map<string, TauriEventHandler>()
    await setupMcpListeners({
      getExplorer: () => ({ navigate, setFocusedPane: setFocusedPaneMock }) as unknown as ExplorerAPI,
      dispatch: vi.fn(),
      listenTauri: (event, handler) => {
        handlers.set(event, handler)
        return Promise.resolve()
      },
      isAiEnabled: () => false,
    })
    return handlers
  }

  it('resolves the path, navigates with the resolved location, focuses the pane, and replies ok', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: { volumeId: 'root', path: '/Library' } })
    const navigate = vi.fn((): NavigateResult => ({ status: 'started', settled: Promise.resolve() }))
    const handlers = await setupWithExplorer(navigate)

    getHandler(handlers, 'mcp-nav-to-path')({ payload: { pane: 'left', path: '/Library', requestId: 'req-1' } })
    await flushAsyncWork()

    expect(resolveLocationMock).toHaveBeenCalledWith('/Library')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { goTo: { volumeId: 'root', path: '/Library' } },
      source: 'mcp',
    })
    // Focus follows the navigated pane so FE focus matches the backend store.
    expect(setFocusedPaneMock).toHaveBeenCalledWith('left')
    expect(emit).toHaveBeenCalledWith('mcp-response', { requestId: 'req-1', ok: true })
  })

  it('does NOT shift focus when the navigate is refused', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: { volumeId: 'network', path: 'smb://h/s' } })
    const navigate = vi.fn(
      (): NavigateResult => ({
        status: 'refused',
        reason: { kind: 'on-network-volume', message: 'nope' },
      }),
    )
    const handlers = await setupWithExplorer(navigate)

    getHandler(handlers, 'mcp-nav-to-path')({ payload: { pane: 'right', path: 'smb://h/s', requestId: 'req-r' } })
    await flushAsyncWork()

    expect(setFocusedPaneMock).not.toHaveBeenCalled()
  })

  it('replies ok:false WITHOUT navigating when the path cannot be resolved', async () => {
    resolveLocationMock.mockResolvedValue({ ok: false, reason: 'no-volume' })
    const navigate = vi.fn((): NavigateResult => ({ status: 'started', settled: Promise.resolve() }))
    const handlers = await setupWithExplorer(navigate)

    getHandler(handlers, 'mcp-nav-to-path')({ payload: { pane: 'left', path: '/Volumes/Gone/x', requestId: 'req-2' } })
    await flushAsyncWork()

    expect(navigate).not.toHaveBeenCalled()
    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-2',
      ok: false,
      error: "Couldn't reach that location's drive. It might be disconnected.",
    })
  })

  it('forwards a synchronous navigate refusal message verbatim (the narrowed on-network refusal)', async () => {
    // An smb:// target maps back to the virtual `network` id, so a network pane
    // still refuses it — and the exact string is the byte-for-byte contract.
    resolveLocationMock.mockResolvedValue({ ok: true, location: { volumeId: 'network', path: 'smb://h/s' } })
    const navigate = vi.fn(
      (): NavigateResult => ({
        status: 'refused',
        reason: {
          kind: 'on-network-volume',
          message: 'Pane is on the Network volume. Use select_volume to switch to a local volume first.',
        },
      }),
    )
    const handlers = await setupWithExplorer(navigate)

    getHandler(handlers, 'mcp-nav-to-path')({ payload: { pane: 'left', path: 'smb://h/s', requestId: 'req-3' } })
    await flushAsyncWork()

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-3',
      ok: false,
      error: 'Pane is on the Network volume. Use select_volume to switch to a local volume first.',
    })
  })
})
