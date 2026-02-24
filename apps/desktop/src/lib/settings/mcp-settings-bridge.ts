/**
 * MCP Settings Bridge - syncs settings state with the Rust backend for MCP tools.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { buildSectionTree, getSetting, setSetting, settingsRegistry, isModified } from '$lib/settings'
import type { SettingId, SettingsValues } from '$lib/settings'
import {
    getEffectiveShortcuts,
    getDefaultShortcuts,
    isShortcutModified,
    setShortcut,
    removeShortcut,
    resetShortcut,
} from '$lib/shortcuts'
import { commands } from '$lib/commands/command-registry'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('mcp-settings')

interface SettingsSection {
    name: string
    path: string[]
    subsections: SettingsSection[]
}

interface SettingItem {
    id: string
    label: string
    description: string
    settingType: string
    value: unknown
    defaultValue: unknown
    isModified: boolean
    constraints?: unknown
}

interface ShortcutCommand {
    id: string
    name: string
    scope: string
    shortcuts: string[]
    defaultShortcuts: string[]
    isModified: boolean
}

/**
 * Convert the section tree to the format expected by the Rust backend.
 */
function convertSectionTree(sections: ReturnType<typeof buildSectionTree>): SettingsSection[] {
    return sections.map((section) => ({
        name: section.name,
        path: section.path,
        subsections: convertSectionTree(section.subsections),
    }))
}

/**
 * Get all settings for the current section.
 */
function getSettingsForSection(sectionPath: string[]): SettingItem[] {
    const items: SettingItem[] = []

    for (const setting of settingsRegistry) {
        // Check if this setting belongs to the current section
        if (
            sectionPath.length <= setting.section.length &&
            sectionPath.every((part, i) => setting.section[i] === part)
        ) {
            const id = setting.id as SettingId
            const value = getSetting(id)
            items.push({
                id: setting.id,
                label: setting.label,
                description: setting.description,
                settingType: setting.type,
                value,
                defaultValue: setting.default,
                isModified: isModified(id),
                constraints: setting.constraints,
            })
        }
    }

    return items
}

/**
 * Get all shortcut commands.
 */
function getAllShortcuts(): ShortcutCommand[] {
    return commands.map((cmd) => ({
        id: cmd.id,
        name: cmd.name,
        scope: cmd.scope,
        shortcuts: getEffectiveShortcuts(cmd.id),
        defaultShortcuts: getDefaultShortcuts(cmd.id),
        isModified: isShortcutModified(cmd.id),
    }))
}

/**
 * Sync the current settings state to the Rust backend.
 */
export async function syncSettingsState(selectedSection: string[]): Promise<void> {
    try {
        const sectionTree = buildSectionTree()

        // Add special sections that aren't in the registry tree
        const sections = convertSectionTree(sectionTree)
        sections.push({
            name: 'Keyboard shortcuts',
            path: ['Keyboard shortcuts'],
            subsections: [],
        })
        sections.push({
            name: 'Advanced',
            path: ['Advanced'],
            subsections: [],
        })

        await invoke('mcp_update_settings_sections', { sections })
        await invoke('mcp_update_settings_section', { section: selectedSection })
        await invoke('mcp_update_current_settings', { settings: getSettingsForSection(selectedSection) })
        await invoke('mcp_update_shortcuts', { shortcuts: getAllShortcuts() })

        log.debug('Synced settings state to backend')
    } catch (error) {
        log.error('Failed to sync settings state: {error}', { error })
    }
}

/**
 * Notify the backend that the settings window is open/closed.
 */
export async function notifySettingsWindowOpen(isOpen: boolean): Promise<void> {
    try {
        await invoke('mcp_update_settings_open', { isOpen })
        log.debug('Notified backend of settings window state: {isOpen}', { isOpen })
    } catch (error) {
        log.error('Failed to notify settings window state: {error}', { error })
    }
}

// Event handlers
let unlistenFns: UnlistenFn[] = []

interface McpSelectSectionPayload {
    sectionPath: string[]
}

interface McpSetValuePayload {
    settingId: string
    value: unknown
}

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
 * Set up MCP event listeners for the settings window.
 */
export async function setupMcpEventListeners(
    onSectionSelect: (sectionPath: string[]) => void,
    onSettingChanged: () => void,
): Promise<void> {
    // Listen for close request
    const unlistenClose = await listen('mcp-settings-close', () => {
        log.debug('MCP requested settings window close')
        void getCurrentWindow().close()
    })
    unlistenFns.push(unlistenClose)

    // Listen for section selection
    const unlistenSection = await listen<McpSelectSectionPayload>('mcp-settings-select-section', (event) => {
        log.debug('MCP requested section select: {section}', { section: event.payload.sectionPath.join(' > ') })
        onSectionSelect(event.payload.sectionPath)
    })
    unlistenFns.push(unlistenSection)

    // Listen for value changes
    const unlistenValue = await listen<McpSetValuePayload>('mcp-settings-set-value', (event) => {
        const { settingId, value } = event.payload
        log.debug('MCP requested setting change: {settingId} = {value}', { settingId, value })

        try {
            setSetting(settingId as SettingId, value as SettingsValues[SettingId])
            onSettingChanged()
        } catch (error) {
            log.error('Failed to set setting via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenValue)

    // Listen for shortcut set
    const unlistenShortcutsSet = await listen<McpShortcutsSetPayload>('mcp-shortcuts-set', (event) => {
        const { commandId, index, shortcut } = event.payload
        log.debug('MCP requested shortcut set: {commandId}[{index}] = {shortcut}', { commandId, index, shortcut })

        try {
            setShortcut(commandId, index, shortcut)
            onSettingChanged()
        } catch (error) {
            log.error('Failed to set shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenShortcutsSet)

    // Listen for shortcut remove
    const unlistenShortcutsRemove = await listen<McpShortcutsRemovePayload>('mcp-shortcuts-remove', (event) => {
        const { commandId, index } = event.payload
        log.debug('MCP requested shortcut remove: {commandId}[{index}]', { commandId, index })

        try {
            removeShortcut(commandId, index)
            onSettingChanged()
        } catch (error) {
            log.error('Failed to remove shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenShortcutsRemove)

    // Listen for shortcut reset
    const unlistenShortcutsReset = await listen<McpShortcutsResetPayload>('mcp-shortcuts-reset', (event) => {
        const { commandId } = event.payload
        log.debug('MCP requested shortcut reset: {commandId}', { commandId })

        try {
            resetShortcut(commandId)
            onSettingChanged()
        } catch (error) {
            log.error('Failed to reset shortcut via MCP: {error}', { error })
        }
    })
    unlistenFns.push(unlistenShortcutsReset)

    log.debug('MCP event listeners set up')
}

/**
 * Clean up MCP event listeners.
 */
export function cleanupMcpEventListeners(): void {
    for (const unlisten of unlistenFns) {
        unlisten()
    }
    unlistenFns = []
    log.debug('MCP event listeners cleaned up')
}
