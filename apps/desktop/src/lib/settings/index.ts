/**
 * Settings module public API.
 */

// Types
export type {
    DateTimeFormat,
    DensityValues,
    DurationUnit,
    EnumOption,
    DirectorySortMode,
    ExtensionChangePolicy,
    FileSizeFormat,
    NetworkTimeoutMode,
    SettingConstraints,
    SettingDefinition,
    SettingId,
    SettingSearchResult,
    SettingsValues,
    SettingType,
    ThemeMode,
    UiDensity,
} from './types'

export { densityMappings, formatDuration, SettingValidationError } from './types'

// Registry
export {
    buildSectionTree,
    getAdvancedSettings,
    getDefaultValue,
    getSettingDefinition,
    getSettingsInSection,
    settingsRegistry,
    validateSettingValue,
} from './settings-registry'

export type { SettingsSection } from './settings-registry'

// Store
export {
    forceSave,
    getSetting,
    initializeSettings,
    isModified,
    onSettingChange,
    onSpecificSettingChange,
    resetSetting,
    setSetting,
} from './settings-store'

// Search
export {
    clearSearchIndex,
    getMatchingSections,
    highlightMatches,
    searchAdvancedSettings,
    searchSettings,
    sectionHasMatches,
} from './settings-search'

// Network settings helpers
export { getMountTimeoutMs, getNetworkTimeoutMs, getShareCacheTtlMs } from './network-settings'
