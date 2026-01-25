/**
 * Settings system type definitions.
 * See docs/specs/settings.md for full specification.
 */

// ============================================================================
// Core Types
// ============================================================================

export type SettingType = 'boolean' | 'number' | 'string' | 'enum' | 'duration'

export type DurationUnit = 'ms' | 's' | 'min' | 'h' | 'd'

export interface EnumOption {
    value: string | number
    label: string
    description?: string
}

export interface SettingConstraints {
    // For 'number' type
    min?: number
    max?: number
    step?: number
    sliderStops?: number[] // Specific values the slider snaps to

    // For 'enum' type
    options?: EnumOption[]
    allowCustom?: boolean
    customMin?: number
    customMax?: number

    // For 'duration' type
    unit?: DurationUnit
    minMs?: number
    maxMs?: number
}

export interface SettingDefinition {
    // Identity
    id: string
    section: string[]

    // Display
    label: string
    description: string
    keywords: string[]

    // Type and constraints
    type: SettingType
    default: unknown
    constraints?: SettingConstraints

    // Behavior
    requiresRestart?: boolean
    disabled?: boolean
    disabledReason?: string

    // UI hints
    component?: 'switch' | 'select' | 'radio' | 'slider' | 'toggle-group' | 'number-input' | 'text-input' | 'duration'
    showInAdvanced?: boolean
}

// ============================================================================
// Setting Value Types (for type-safe access)
// ============================================================================

export type UiDensity = 'compact' | 'comfortable' | 'spacious'
export type FileSizeFormat = 'binary' | 'si'
export type DateTimeFormat = 'system' | 'iso' | 'short' | 'custom'
export type NetworkTimeoutMode = 'normal' | 'slow' | 'custom'
export type ThemeMode = 'light' | 'dark' | 'system'

export interface SettingsValues {
    // Appearance
    'appearance.uiDensity': UiDensity
    'appearance.useAppIconsForDocuments': boolean
    'appearance.fileSizeFormat': FileSizeFormat
    'appearance.dateTimeFormat': DateTimeFormat
    'appearance.customDateTimeFormat': string

    // File operations
    'fileOperations.confirmBeforeDelete': boolean
    'fileOperations.deletePermanently': boolean
    'fileOperations.progressUpdateInterval': number
    'fileOperations.maxConflictsToShow': number

    // Updates
    'updates.autoCheck': boolean

    // Network
    'network.shareCacheDuration': number
    'network.timeoutMode': NetworkTimeoutMode
    'network.customTimeout': number

    // Theme
    'theme.mode': ThemeMode

    // Developer
    'developer.mcpEnabled': boolean
    'developer.mcpPort': number
    'developer.verboseLogging': boolean

    // Advanced
    'advanced.dragThreshold': number
    'advanced.prefetchBufferSize': number
    'advanced.virtualizationBufferRows': number
    'advanced.virtualizationBufferColumns': number
    'advanced.fileWatcherDebounce': number
    'advanced.serviceResolveTimeout': number
    'advanced.mountTimeout': number
    'advanced.updateCheckInterval': number
}

export type SettingId = keyof SettingsValues

// ============================================================================
// Search Result Types
// ============================================================================

export interface SettingSearchResult {
    setting: SettingDefinition
    matchedIndices: number[]
    searchableText: string
}

// ============================================================================
// Validation Error
// ============================================================================

export class SettingValidationError extends Error {
    constructor(
        public settingId: string,
        public reason: string,
    ) {
        super(`Invalid value for setting '${settingId}': ${reason}`)
        this.name = 'SettingValidationError'
    }
}

// ============================================================================
// UI Density Mappings
// ============================================================================

export interface DensityValues {
    rowHeight: number
    iconSize: number
    spacing: number
}

export const densityMappings: Record<UiDensity, DensityValues> = {
    compact: { rowHeight: 16, iconSize: 24, spacing: 2 },
    comfortable: { rowHeight: 20, iconSize: 32, spacing: 4 },
    spacious: { rowHeight: 28, iconSize: 40, spacing: 8 },
}

// ============================================================================
// Duration Conversion Helpers
// ============================================================================

const MS_PER_UNIT: Record<DurationUnit, number> = {
    ms: 1,
    s: 1000,
    min: 60 * 1000,
    h: 60 * 60 * 1000,
    d: 24 * 60 * 60 * 1000,
}

export function toMilliseconds(value: number, unit: DurationUnit): number {
    return value * MS_PER_UNIT[unit]
}

export function fromMilliseconds(ms: number, unit: DurationUnit): number {
    return ms / MS_PER_UNIT[unit]
}

export function formatDuration(ms: number): string {
    if (ms < 1000) return ms.toString() + 'ms'
    if (ms < 60000) return (ms / 1000).toString() + 's'
    if (ms < 3600000) return (ms / 60000).toString() + 'min'
    if (ms < 86400000) return (ms / 3600000).toString() + 'h'
    return (ms / 86400000).toString() + 'd'
}
