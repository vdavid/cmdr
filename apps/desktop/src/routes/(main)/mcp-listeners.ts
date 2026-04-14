/**
 * MCP event listeners: wires Tauri events from the MCP server to ExplorerAPI methods.
 * Pure plumbing with no business logic.
 */

import type { ViewMode } from '$lib/app-status-store'
import type { ExplorerAPI } from './explorer-api'

export interface McpListenerContext {
    getExplorer: () => ExplorerAPI | undefined
    listenTauri: (event: string, handler: (event: { payload: unknown }) => void) => Promise<void>
}

/** Register all MCP event listeners. Call from onMount after listenTauri is ready. */
export async function setupMcpListeners(ctx: McpListenerContext): Promise<void> {
    const { listenTauri, getExplorer } = ctx

    await listenTauri('mcp-key', (event) => {
        const { key } = event.payload as { key: string }
        if (key === 'GoBack') {
            getExplorer()?.navigate('back')
        } else if (key === 'GoForward') {
            getExplorer()?.navigate('forward')
        } else {
            getExplorer()?.sendKeyToFocusedPane(key)
        }
    })

    await listenTauri('menu-sort', (event) => {
        const { action, value } = event.payload as { action: string; value: string }
        if (action === 'sortBy') {
            const column = value as 'name' | 'extension' | 'size' | 'modified' | 'created'
            getExplorer()?.setSortColumn(column)
        } else if (action === 'sortOrder') {
            const order = value as 'asc' | 'desc' | 'toggle'
            getExplorer()?.setSortOrder(order)
        }
    })

    await listenTauri('mcp-sort', (event) => {
        const { pane, by, order } = event.payload as { pane: 'left' | 'right'; by: string; order: string }
        const column = by === 'ext' ? 'extension' : (by as 'name' | 'extension' | 'size' | 'modified' | 'created')
        void getExplorer()?.setSort(column, order as 'asc' | 'desc', pane)
    })

    await listenTauri('mcp-volume-select', (event) => {
        const { pane, name } = event.payload as { pane: 'left' | 'right'; name: string }
        void getExplorer()?.selectVolumeByName(pane, name)
    })

    await listenTauri('mcp-select', (event) => {
        const { pane, start, count, mode } = event.payload as {
            pane: 'left' | 'right'
            start: number
            count: number | 'all'
            mode: string
        }
        getExplorer()?.handleMcpSelect(pane, start, count, mode)
    })

    await listenTauri('mcp-nav-to-path', (event) => {
        const { pane, path, requestId } = event.payload as {
            pane: 'left' | 'right'
            path: string
            requestId?: string
        }
        const explorerRef = getExplorer()
        // explorerRef may be null during HMR — skip silently, let the backend timeout handle it
        if (!explorerRef) return
        const result = explorerRef.navigateToPath(pane, path)
        if (requestId) {
            void (async () => {
                const { emit } = await import('@tauri-apps/api/event')
                if (typeof result === 'string') {
                    // Synchronous error (pane not available, wrong volume, etc.)
                    await emit('mcp-response', { requestId, ok: false, error: result })
                } else {
                    // Promise — wait for directory listing to complete
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

    await listenTauri('mcp-move-cursor', (event) => {
        const { pane, to, requestId } = event.payload as { pane: 'left' | 'right'; to: number | string; requestId: string }
        void (async () => {
            const { emit } = await import('@tauri-apps/api/event')
            try {
                await getExplorer()?.moveCursor(pane, to)
                await emit('mcp-response', { requestId, ok: true })
            } catch (e) {
                const error = e instanceof Error ? e.message : String(e)
                await emit('mcp-response', { requestId, ok: false, error })
            }
        })()
    })

    await listenTauri('mcp-scroll-to', (event) => {
        const { pane, index } = event.payload as { pane: 'left' | 'right'; index: number }
        getExplorer()?.scrollTo(pane, index)
    })

    await listenTauri('mcp-set-view-mode', (event) => {
        const { pane, mode } = event.payload as { pane: 'left' | 'right'; mode: string }
        getExplorer()?.setViewMode(mode as ViewMode, pane)
    })

    await listenTauri('mcp-refresh', () => {
        getExplorer()?.refreshPane()
    })

    await listenTauri('mcp-copy', (event) => {
        const { autoConfirm, onConflict } = event.payload as {
            autoConfirm?: boolean
            onConflict?: string
        }
        void getExplorer()?.openCopyDialog(autoConfirm, onConflict)
    })

    await listenTauri('mcp-move', (event) => {
        const { autoConfirm, onConflict } = event.payload as {
            autoConfirm?: boolean
            onConflict?: string
        }
        void getExplorer()?.openMoveDialog(autoConfirm, onConflict)
    })

    await listenTauri('mcp-mkdir', () => {
        void getExplorer()?.openNewFolderDialog()
    })

    await listenTauri('mcp-mkfile', () => {
        void getExplorer()?.openNewFileDialog()
    })

    await listenTauri('mcp-delete', (event) => {
        const { autoConfirm } = event.payload as { autoConfirm?: boolean }
        void getExplorer()?.openDeleteDialog(false, autoConfirm)
    })

    await listenTauri('mcp-confirm-dialog', (event) => {
        const { type, onConflict } = event.payload as {
            type: string
            onConflict?: string
        }
        getExplorer()?.confirmDialog(type, onConflict)
    })
}
