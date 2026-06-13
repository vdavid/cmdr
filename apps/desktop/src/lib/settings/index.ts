/**
 * Settings module public API.
 */

// Types
export type {
  AiLocalContextSize,
  AiProvider,
  AppColor,
  DateTimeFormat,
  DensityValues,
  DurationUnit,
  EnumOption,
  DirectorySortMode,
  SizeDisplayMode,
  BriefColumnWidthMode,
  ExtensionChangePolicy,
  FileSizeFormat,
  FileSizeUnit,
  NetworkTimeoutMode,
  SettingConstraints,
  SettingDefinition,
  SettingId,
  SettingSearchResult,
  SettingsValues,
  SettingType,
  SizeColorsPalette,
  DateColorsPalette,
  ThemeMode,
  UiDensity,
  VolumeTintColor,
} from './types'

export { densityMappings, formatDuration, SettingValidationError, VOLUME_TINT_COLORS } from './types'

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
  deleteRawStoreKeys,
  forceSave,
  getRawStoreValue,
  getSetting,
  initializeSettings,
  isModified,
  onSettingChange,
  onSpecificSettingChange,
  resetSetting,
  seedSettingForE2E,
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

// Cloud provider presets
export {
  cloudProviderPresets,
  getCloudProvider,
  getProviderConfigs,
  setProviderConfig,
  resolveCloudConfig,
} from './cloud-providers'
export type { CloudProviderPreset, CloudProviderConfig } from './cloud-providers'

// Network settings helpers
export { getMountTimeoutMs, getNetworkTimeoutMs, getShareCacheTtlMs } from './network-settings'

// MCP main bridge (settings event handlers for the main window)
export { setupMcpMainBridge, cleanupMcpMainBridge } from './mcp-main-bridge'

// Restricted-settings bridge (persists viewer-originated changes in the main window)
export { setupRestrictedSettingsBridge, cleanupRestrictedSettingsBridge } from './restricted-settings-bridge'
