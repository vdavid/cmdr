/**
 * MCP Shortcuts Listener - handles shortcut changes from MCP tools in the main window.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { setShortcut, removeShortcut, resetShortcut } from './shortcuts-store'
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('mcp-shortcuts')

let unlistenFns: UnlistenFn[] = []

interface McpShortcutsSetPayload {
    commandId: string
    index: number
    shortcut: string
}

interface McpShortcutsRemovePayload {
    commandId: string
    index: number
}

interface McpShortcutsResetPayload {
    commandId: string
}

/**
 * Set up MCP shortcuts event listeners for the main window.
 * These allow MCP tools to modify shortcuts even when the settings window is closed.
 */
export async function setupMcpShortcutsListener(): Promise<void> {
    // Listen for shortcut set
    const unlistenSet = await listen<McpShortcutsSetPayload>('mcp-shortcuts-set', (event) => {
        const { commandId, index, shortcut } = event.payload
        log.debug('MCP requested shortcut set: {commandId}[{index}] = {shortcut}', { commandId, index, shortcut })

        try {
            setShortcut(commandId, index, shortcut)
        } catch (error) {
            log.error('Failed to set shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenSet)

    // Listen for shortcut remove
    const unlistenRemove = await listen<McpShortcutsRemovePayload>('mcp-shortcuts-remove', (event) => {
        const { commandId, index } = event.payload
        log.debug('MCP requested shortcut remove: {commandId}[{index}]', { commandId, index })

        try {
            removeShortcut(commandId, index)
        } catch (error) {
            log.error('Failed to remove shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenRemove)

    // Listen for shortcut reset
    const unlistenReset = await listen<McpShortcutsResetPayload>('mcp-shortcuts-reset', (event) => {
        const { commandId } = event.payload
        log.debug('MCP requested shortcut reset: {commandId}', { commandId })

        try {
            resetShortcut(commandId)
        } catch (error) {
            log.error('Failed to reset shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenReset)

    log.debug('MCP shortcuts listeners set up in main window')
}

/**
 * Clean up MCP shortcuts event listeners.
 */
export function cleanupMcpShortcutsListener(): void {
    for (const unlisten of unlistenFns) {
        unlisten()
    }
    unlistenFns = []
    log.debug('MCP shortcuts listeners cleaned up')
}
