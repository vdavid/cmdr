/**
 * Settings module public API.
 */

// Types
export type {
  AiProvider,
  AppColor,
  DateTimeFormat,
  DirectorySortMode,
  SizeDisplayMode,
  BriefColumnWidthMode,
  ExtensionChangePolicy,
  FileSizeFormat,
  FileSizeUnit,
  FullDiskAccessChoice,
  SettingId,
  SettingsValues,
  SizeColorsPalette,
  DateColorsPalette,
  ThemeMode,
  UiDensity,
  VolumeTintColor,
} from './types'

export {
  densityMappings,
  durationValueToMs,
  formatDurationSetting,
  msToDurationValue,
  VOLUME_TINT_COLORS,
} from './types'

// Registry
export { buildSectionTree, getAdvancedSettings, getDefaultValue, getSettingDefinition } from './settings-registry'

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

// Cloud provider presets
export {
  cloudProviderPresets,
  getCloudProvider,
  getProviderConfigs,
  setProviderConfig,
  resolveCloudConfig,
} from './cloud-providers'

// MCP main bridge (settings event handlers for the main window)
export { setupMcpMainBridge, cleanupMcpMainBridge } from './mcp-main-bridge'

// Restricted-settings bridge (persists viewer-originated changes in the main window)
export { setupRestrictedSettingsBridge, cleanupRestrictedSettingsBridge } from './restricted-settings-bridge'
