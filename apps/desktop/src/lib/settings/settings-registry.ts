/**
 * Settings registry - single source of truth for all settings.
 * See docs/specs/settings.md for full specification.
 */

import type { SettingDefinition, SettingId, SettingsValues } from './types'
import { SettingValidationError } from './types'

// ============================================================================
// Settings Definitions
// ============================================================================

export const settingsRegistry: SettingDefinition[] = [
    // ========================================================================
    // General › Appearance
    // ========================================================================
    {
        id: 'appearance.uiDensity',
        section: ['General', 'Appearance'],
        label: 'UI density',
        description: 'Controls the spacing and size of UI elements throughout the app.',
        keywords: ['compact', 'comfortable', 'spacious', 'size', 'spacing', 'dense'],
        type: 'enum',
        default: 'comfortable',
        component: 'toggle-group',
        constraints: {
            options: [
                { value: 'compact', label: 'Compact' },
                { value: 'comfortable', label: 'Comfortable' },
                { value: 'spacious', label: 'Spacious' },
            ],
        },
    },
    {
        id: 'appearance.useAppIconsForDocuments',
        section: ['General', 'Appearance'],
        label: 'Use app icons for documents',
        description:
            "Show the app's icon for documents instead of generic file type icons. More colorful but slightly slower.",
        keywords: ['icon', 'document', 'file', 'app', 'colorful', 'finder'],
        type: 'boolean',
        default: true,
        component: 'switch',
    },
    {
        id: 'appearance.fileSizeFormat',
        section: ['General', 'Appearance'],
        label: 'File size format',
        description: 'How to display file sizes in the file list.',
        keywords: ['size', 'bytes', 'binary', 'decimal', 'kb', 'mb', 'kib', 'mib'],
        type: 'enum',
        default: 'binary',
        component: 'select',
        constraints: {
            options: [
                { value: 'binary', label: 'Binary (KiB, MiB, GiB)', description: '1 KiB = 1024 bytes' },
                { value: 'si', label: 'SI decimal (KB, MB, GB)', description: '1 KB = 1000 bytes' },
            ],
        },
    },
    {
        id: 'appearance.dateTimeFormat',
        section: ['General', 'Appearance'],
        label: 'Date and time format',
        description: 'How to display dates and times in the file list.',
        keywords: ['date', 'time', 'format', 'iso', 'custom', 'timestamp'],
        type: 'enum',
        default: 'system',
        component: 'radio',
        constraints: {
            options: [
                { value: 'system', label: 'System default' },
                { value: 'iso', label: 'ISO 8601', description: 'e.g., 2025-01-25 14:30' },
                { value: 'short', label: 'Short', description: 'e.g., Jan 25, 2:30 PM' },
                { value: 'custom', label: 'Custom...' },
            ],
            allowCustom: true,
        },
    },
    {
        id: 'appearance.customDateTimeFormat',
        section: ['General', 'Appearance'],
        label: 'Custom date/time format',
        description: 'Format string for custom date/time display. Use placeholders like YYYY, MM, DD, HH, mm, ss.',
        keywords: ['custom', 'format', 'date', 'time', 'placeholder'],
        type: 'string',
        default: 'YYYY-MM-DD HH:mm',
        component: 'text-input',
    },

    // ========================================================================
    // General › File operations
    // ========================================================================
    {
        id: 'fileOperations.confirmBeforeDelete',
        section: ['General', 'File operations'],
        label: 'Confirm before delete',
        description: 'Show a confirmation dialog before moving files to trash.',
        keywords: ['confirm', 'delete', 'trash', 'dialog', 'warning'],
        type: 'boolean',
        default: true,
        component: 'switch',
        disabled: true,
        disabledReason: 'Coming soon',
    },
    {
        id: 'fileOperations.deletePermanently',
        section: ['General', 'File operations'],
        label: 'Delete permanently instead of using trash',
        description: 'Bypass trash and delete files immediately. This cannot be undone.',
        keywords: ['permanent', 'delete', 'trash', 'bypass', 'remove'],
        type: 'boolean',
        default: false,
        component: 'switch',
        disabled: true,
        disabledReason: 'Coming soon',
    },
    {
        id: 'fileOperations.progressUpdateInterval',
        section: ['General', 'File operations'],
        label: 'Progress update interval',
        description:
            'How often to refresh progress during file operations. Lower values feel more responsive but use more CPU.',
        keywords: ['progress', 'update', 'interval', 'refresh', 'cpu', 'performance'],
        type: 'number',
        default: 500,
        component: 'slider',
        constraints: {
            min: 50,
            max: 5000,
            step: 50,
            sliderStops: [100, 250, 500, 1000, 2000],
        },
    },
    {
        id: 'fileOperations.maxConflictsToShow',
        section: ['General', 'File operations'],
        label: 'Maximum conflicts to show',
        description: 'Maximum number of file conflicts to display in the preview before an operation.',
        keywords: ['conflict', 'max', 'limit', 'preview', 'operation'],
        type: 'number',
        default: 100,
        component: 'select',
        constraints: {
            options: [
                { value: 1, label: '1' },
                { value: 2, label: '2' },
                { value: 3, label: '3' },
                { value: 5, label: '5' },
                { value: 10, label: '10' },
                { value: 50, label: '50' },
                { value: 100, label: '100' },
                { value: 200, label: '200' },
                { value: 500, label: '500' },
            ],
            allowCustom: true,
            customMin: 1,
            customMax: 1000,
        },
    },

    // ========================================================================
    // General › Updates
    // ========================================================================
    {
        id: 'updates.autoCheck',
        section: ['General', 'Updates'],
        label: 'Automatically check for updates',
        description: 'Periodically check for new versions in the background.',
        keywords: ['update', 'auto', 'check', 'version', 'background'],
        type: 'boolean',
        default: true,
        component: 'switch',
    },

    // ========================================================================
    // Network › SMB/Network shares
    // ========================================================================
    {
        id: 'network.shareCacheDuration',
        section: ['Network', 'SMB/Network shares'],
        label: 'Share cache duration',
        description: 'How long to cache the list of available shares on a server before refreshing.',
        keywords: ['cache', 'smb', 'share', 'network', 'refresh', 'ttl'],
        type: 'duration',
        default: 30000, // 30 seconds in ms
        component: 'select',
        constraints: {
            unit: 's',
            options: [
                { value: 30000, label: '30 seconds' },
                { value: 300000, label: '5 minutes' },
                { value: 3600000, label: '1 hour' },
                { value: 86400000, label: '1 day' },
                { value: 2592000000, label: '30 days' },
            ],
            allowCustom: true,
            customMin: 1000,
            customMax: 2592000000,
        },
    },
    {
        id: 'network.timeoutMode',
        section: ['Network', 'SMB/Network shares'],
        label: 'Network timeout mode',
        description: 'How long to wait when connecting to network shares.',
        keywords: ['timeout', 'network', 'slow', 'vpn', 'connection', 'latency'],
        type: 'enum',
        default: 'normal',
        component: 'radio',
        constraints: {
            options: [
                { value: 'normal', label: 'Normal', description: 'For typical local networks (15s timeout)' },
                {
                    value: 'slow',
                    label: 'Slow network',
                    description: 'For VPNs or high-latency connections (45s timeout)',
                },
                { value: 'custom', label: 'Custom' },
            ],
            allowCustom: true,
        },
    },
    {
        id: 'network.customTimeout',
        section: ['Network', 'SMB/Network shares'],
        label: 'Custom timeout',
        description: 'Custom timeout in seconds for network operations.',
        keywords: ['timeout', 'custom', 'seconds'],
        type: 'number',
        default: 15,
        component: 'number-input',
        constraints: {
            min: 5,
            max: 120,
            step: 1,
        },
    },

    // ========================================================================
    // Themes
    // ========================================================================
    {
        id: 'theme.mode',
        section: ['Themes'],
        label: 'Theme mode',
        description: 'Choose between light, dark, or system-based theme.',
        keywords: ['theme', 'dark', 'light', 'mode', 'appearance', 'color'],
        type: 'enum',
        default: 'system',
        component: 'toggle-group',
        constraints: {
            options: [
                { value: 'light', label: '☀️ Light' },
                { value: 'dark', label: '🌙 Dark' },
                { value: 'system', label: '💻 System' },
            ],
        },
    },

    // ========================================================================
    // Developer › MCP server
    // ========================================================================
    {
        id: 'developer.mcpEnabled',
        section: ['Developer', 'MCP server'],
        label: 'Enable MCP server',
        description: 'Start a Model Context Protocol server for AI assistant integration.',
        keywords: ['mcp', 'server', 'ai', 'assistant', 'protocol', 'model'],
        type: 'boolean',
        default: false,
        component: 'switch',
        requiresRestart: true,
    },
    {
        id: 'developer.mcpPort',
        section: ['Developer', 'MCP server'],
        label: 'Port',
        description: 'The port number for the MCP server. Default: 9224',
        keywords: ['port', 'mcp', 'network'],
        type: 'number',
        default: 9224,
        component: 'number-input',
        constraints: {
            min: 1024,
            max: 65535,
            step: 1,
        },
        requiresRestart: true,
    },

    // ========================================================================
    // Developer › Logging
    // ========================================================================
    {
        id: 'developer.verboseLogging',
        section: ['Developer', 'Logging'],
        label: 'Verbose logging',
        description: 'Log detailed debug information. Useful for troubleshooting. May impact performance.',
        keywords: ['log', 'debug', 'verbose', 'troubleshoot', 'performance'],
        type: 'boolean',
        default: false,
        component: 'switch',
    },

    // ========================================================================
    // Advanced (generated UI)
    // ========================================================================
    {
        id: 'advanced.dragThreshold',
        section: ['Advanced'],
        label: 'Drag threshold',
        description: 'Minimum distance in pixels before a drag operation starts.',
        keywords: ['drag', 'threshold', 'pixel', 'distance'],
        type: 'number',
        default: 5,
        component: 'number-input',
        showInAdvanced: true,
        constraints: {
            min: 1,
            max: 50,
            step: 1,
        },
    },
    {
        id: 'advanced.prefetchBufferSize',
        section: ['Advanced'],
        label: 'Prefetch buffer size',
        description: 'Number of items to prefetch around the visible range for smoother scrolling.',
        keywords: ['prefetch', 'buffer', 'scroll', 'performance'],
        type: 'number',
        default: 200,
        component: 'number-input',
        showInAdvanced: true,
        constraints: {
            min: 50,
            max: 1000,
            step: 50,
        },
    },
    {
        id: 'advanced.virtualizationBufferRows',
        section: ['Advanced'],
        label: 'Virtualization buffer (rows)',
        description: 'Extra rows to render above and below the visible area.',
        keywords: ['virtualization', 'buffer', 'row', 'render'],
        type: 'number',
        default: 20,
        component: 'number-input',
        showInAdvanced: true,
        constraints: {
            min: 5,
            max: 100,
            step: 5,
        },
    },
    {
        id: 'advanced.virtualizationBufferColumns',
        section: ['Advanced'],
        label: 'Virtualization buffer (columns)',
        description: 'Extra columns to render in brief view.',
        keywords: ['virtualization', 'buffer', 'column', 'brief'],
        type: 'number',
        default: 2,
        component: 'number-input',
        showInAdvanced: true,
        constraints: {
            min: 1,
            max: 10,
            step: 1,
        },
    },
    {
        id: 'advanced.fileWatcherDebounce',
        section: ['Advanced'],
        label: 'File watcher debounce',
        description: 'Delay after file system changes before refreshing.',
        keywords: ['watcher', 'debounce', 'refresh', 'delay'],
        type: 'duration',
        default: 200,
        component: 'duration',
        showInAdvanced: true,
        constraints: {
            unit: 'ms',
            minMs: 50,
            maxMs: 2000,
        },
    },
    {
        id: 'advanced.serviceResolveTimeout',
        section: ['Advanced'],
        label: 'Service resolve timeout',
        description: 'Timeout for resolving network services via Bonjour.',
        keywords: ['bonjour', 'resolve', 'timeout', 'mdns'],
        type: 'duration',
        default: 5000,
        component: 'duration',
        showInAdvanced: true,
        constraints: {
            unit: 's',
            minMs: 1000,
            maxMs: 30000,
        },
    },
    {
        id: 'advanced.mountTimeout',
        section: ['Advanced'],
        label: 'Mount timeout',
        description: 'Timeout for mounting network shares.',
        keywords: ['mount', 'timeout', 'network', 'share'],
        type: 'duration',
        default: 20000,
        component: 'duration',
        showInAdvanced: true,
        constraints: {
            unit: 's',
            minMs: 5000,
            maxMs: 120000,
        },
    },
    {
        id: 'advanced.updateCheckInterval',
        section: ['Advanced'],
        label: 'Update check interval',
        description: 'How often to check for updates in the background.',
        keywords: ['update', 'interval', 'background', 'check'],
        type: 'duration',
        default: 3600000, // 60 minutes
        component: 'duration',
        showInAdvanced: true,
        constraints: {
            unit: 'min',
            minMs: 300000, // 5 min
            maxMs: 86400000, // 24 hours
        },
    },
]

// ============================================================================
// Registry Lookup Helpers
// ============================================================================

const registryMap = new Map<string, SettingDefinition>()
for (const setting of settingsRegistry) {
    registryMap.set(setting.id, setting)
}

/**
 * Get the definition for a setting by ID.
 */
export function getSettingDefinition(id: string): SettingDefinition | undefined {
    return registryMap.get(id)
}

/**
 * Get all settings in a section path.
 */
export function getSettingsInSection(sectionPath: string[]): SettingDefinition[] {
    return settingsRegistry.filter((s) => {
        if (s.section.length < sectionPath.length) return false
        return sectionPath.every((part, i) => s.section[i] === part)
    })
}

/**
 * Get all settings marked for the Advanced section.
 */
export function getAdvancedSettings(): SettingDefinition[] {
    return settingsRegistry.filter((s) => s.showInAdvanced)
}

/**
 * Get the default value for a setting.
 */
export function getDefaultValue<K extends SettingId>(id: K): SettingsValues[K] {
    const def = registryMap.get(id)
    if (!def) throw new Error(`Unknown setting: ${id}`)
    return def.default as SettingsValues[K]
}

// ============================================================================
// Validation
// ============================================================================

/**
 * Validate a value against a setting's constraints.
 * Throws SettingValidationError if invalid.
 */
export function validateSettingValue(id: string, value: unknown): void {
    const def = registryMap.get(id)
    if (!def) {
        throw new SettingValidationError(id, 'Unknown setting')
    }

    // Type checking
    switch (def.type) {
        case 'boolean':
            if (typeof value !== 'boolean') {
                throw new SettingValidationError(id, `Expected boolean, got ${typeof value}`)
            }
            break

        case 'number':
        case 'duration':
            if (typeof value !== 'number') {
                throw new SettingValidationError(id, `Expected number, got ${typeof value}`)
            }
            if (!Number.isFinite(value)) {
                throw new SettingValidationError(id, 'Value must be a finite number')
            }
            validateNumberConstraints(id, value, def)
            break

        case 'string':
            if (typeof value !== 'string') {
                throw new SettingValidationError(id, `Expected string, got ${typeof value}`)
            }
            break

        case 'enum':
            validateEnumValue(id, value, def)
            break
    }
}

function validateNumberConstraints(id: string, value: number, def: SettingDefinition): void {
    const c = def.constraints
    if (!c) return

    // For duration type, check minMs/maxMs
    if (def.type === 'duration') {
        if (c.minMs !== undefined && value < c.minMs) {
            throw new SettingValidationError(id, `Value ${String(value)}ms is below minimum ${String(c.minMs)}ms`)
        }
        if (c.maxMs !== undefined && value > c.maxMs) {
            throw new SettingValidationError(id, `Value ${String(value)}ms exceeds maximum ${String(c.maxMs)}ms`)
        }
        return
    }

    // For number type, check min/max
    if (c.min !== undefined && value < c.min) {
        throw new SettingValidationError(id, `Value ${String(value)} is below minimum ${String(c.min)}`)
    }
    if (c.max !== undefined && value > c.max) {
        throw new SettingValidationError(id, `Value ${String(value)} exceeds maximum ${String(c.max)}`)
    }
}

function validateEnumValue(id: string, value: unknown, def: SettingDefinition): void {
    const c = def.constraints
    if (!c?.options) return

    const validValues = c.options.map((o) => o.value)

    // Check if it's one of the predefined options
    if (validValues.includes(value as string | number)) {
        return
    }

    // Check if custom values are allowed
    if (c.allowCustom && typeof value === 'number') {
        if (c.customMin !== undefined && value < c.customMin) {
            throw new SettingValidationError(
                id,
                `Custom value ${String(value)} is below minimum ${String(c.customMin)}`,
            )
        }
        if (c.customMax !== undefined && value > c.customMax) {
            throw new SettingValidationError(id, `Custom value ${String(value)} exceeds maximum ${String(c.customMax)}`)
        }
        return
    }

    throw new SettingValidationError(id, `Invalid value '${String(value)}'. Valid options: ${validValues.join(', ')}`)
}

// ============================================================================
// Section Tree Building
// ============================================================================

export interface SettingsSection {
    name: string
    path: string[]
    subsections: SettingsSection[]
    settings: SettingDefinition[]
}

/**
 * Build a hierarchical tree structure from the flat settings registry.
 */
export function buildSectionTree(): SettingsSection[] {
    const root: SettingsSection[] = []
    const sectionMap = new Map<string, SettingsSection>()

    for (const setting of settingsRegistry) {
        if (setting.showInAdvanced) continue // Advanced settings are handled separately

        let currentLevel = root
        let currentPath: string[] = []

        for (let i = 0; i < setting.section.length; i++) {
            const sectionName = setting.section[i]
            currentPath = [...currentPath, sectionName]
            const pathKey = currentPath.join('/')

            let section = sectionMap.get(pathKey)
            if (!section) {
                section = {
                    name: sectionName,
                    path: [...currentPath],
                    subsections: [],
                    settings: [],
                }
                sectionMap.set(pathKey, section)
                currentLevel.push(section)
            }

            if (i === setting.section.length - 1) {
                section.settings.push(setting)
            } else {
                currentLevel = section.subsections
            }
        }
    }

    return root
}
