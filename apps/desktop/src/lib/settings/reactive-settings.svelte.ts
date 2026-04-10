/**
 * Reactive settings state for Svelte components.
 * Provides $state-based values that update immediately when settings change.
 */

import {
  getSetting,
  onSettingChange,
  initializeSettings,
  type UiDensity,
  type DateTimeFormat,
  type FileSizeFormat,
  type DirectorySortMode,
  type SizeDisplayMode,
  type AppColor,
  densityMappings,
} from '$lib/settings'
import { formatDateTimeWithFormat, formatFileSizeWithFormat } from './format-utils'
import { getAppLogger } from '$lib/logging/logger'
import { clearExtensionIconCache } from '$lib/icon-cache'

const log = getAppLogger('reactive-settings')

// Reactive state for settings that affect UI rendering
let uiDensity = $state<UiDensity>('comfortable')
let dateTimeFormat = $state<DateTimeFormat>('iso')
let customDateTimeFormat = $state<string>('YYYY-MM-DD HH:mm')
let fileSizeFormat = $state<FileSizeFormat>('binary')
let useAppIconsForDocuments = $state<boolean>(true)
let directorySortMode = $state<DirectorySortMode>('likeFiles')
let appColor = $state<AppColor>('cmdr-gold')
let sizeDisplay = $state<SizeDisplayMode>('smart')
let sizeMismatchWarning = $state<boolean>(true)
let stripedRows = $state<boolean>(false)

let initialized = false
let unsubscribe: (() => void) | undefined

/**
 * Initialize reactive settings. Call once on app startup.
 */
export async function initReactiveSettings(): Promise<void> {
  if (initialized) return

  log.debug('Initializing reactive settings')

  try {
    await initializeSettings()

    // Load initial values
    uiDensity = getSetting('appearance.uiDensity')
    dateTimeFormat = getSetting('appearance.dateTimeFormat')
    customDateTimeFormat = getSetting('appearance.customDateTimeFormat')
    fileSizeFormat = getSetting('appearance.fileSizeFormat')
    useAppIconsForDocuments = getSetting('appearance.useAppIconsForDocuments')
    directorySortMode = getSetting('listing.directorySortMode')
    appColor = getSetting('appearance.appColor')
    sizeDisplay = getSetting('listing.sizeDisplay')
    sizeMismatchWarning = getSetting('listing.sizeMismatchWarning')
    stripedRows = getSetting('listing.stripedRows')

    // Subscribe to changes (including cross-window changes)
    unsubscribe = onSettingChange((id, value) => {
      log.debug('Received setting change: {id} = {value}', { id, value })

      switch (id) {
        case 'appearance.uiDensity':
          uiDensity = value as UiDensity
          break
        case 'appearance.dateTimeFormat':
          dateTimeFormat = value as DateTimeFormat
          break
        case 'appearance.customDateTimeFormat':
          customDateTimeFormat = value as string
          break
        case 'appearance.fileSizeFormat':
          fileSizeFormat = value as FileSizeFormat
          break
        case 'appearance.useAppIconsForDocuments':
          useAppIconsForDocuments = value as boolean
          // Clear the icon cache so icons are re-fetched with the new setting
          void clearExtensionIconCache()
          break
        case 'listing.directorySortMode':
          directorySortMode = value as DirectorySortMode
          break
        case 'appearance.appColor':
          appColor = value as AppColor
          break
        case 'listing.sizeDisplay':
          sizeDisplay = value as SizeDisplayMode
          break
        case 'listing.sizeMismatchWarning':
          sizeMismatchWarning = value as boolean
          break
        case 'listing.stripedRows':
          stripedRows = value as boolean
          break
      }
    })

    initialized = true
    log.debug('Reactive settings initialized')
  } catch (error) {
    log.error('Failed to initialize reactive settings: {error}', { error })
  }
}

/**
 * Cleanup reactive settings.
 */
export function cleanupReactiveSettings(): void {
  unsubscribe?.()
  unsubscribe = undefined
  initialized = false
}

// ============================================================================
// Getters for reactive values (use these in components)
// ============================================================================

/** Get current row height based on density */
export function getRowHeight(): number {
  return densityMappings[uiDensity].rowHeight
}

/** Get whether the current density is compact */
export function getIsCompactDensity(): boolean {
  return uiDensity === 'compact'
}

/** Get current "use app icons for documents" setting */
export function getUseAppIconsForDocuments(): boolean {
  return useAppIconsForDocuments
}

/** Get current directory sort mode */
export function getDirectorySortMode(): DirectorySortMode {
  return directorySortMode
}

/** Whether the user has selected Cmdr gold as their app color */
export function getIsCmdrGold(): boolean {
  return appColor === 'cmdr-gold'
}

/** Get current size display mode (smart, logical, or physical) */
export function getSizeDisplayMode(): SizeDisplayMode {
  return sizeDisplay
}

/** Get whether the size mismatch warning icon is enabled */
export function getSizeMismatchWarning(): boolean {
  return sizeMismatchWarning
}

/** Get whether striped rows are enabled */
export function getStripedRows(): boolean {
  return stripedRows
}

// ============================================================================
// Formatting utilities that use reactive settings
// ============================================================================

/**
 * Format a timestamp according to current settings.
 * @param timestamp Unix timestamp in seconds
 */
export function formatDateTime(timestamp: number | undefined): string {
  return formatDateTimeWithFormat(timestamp, dateTimeFormat, customDateTimeFormat)
}

/**
 * Format bytes as human-readable string according to current settings.
 * @param bytes Number of bytes
 */
export function formatFileSize(bytes: number): string {
  return formatFileSizeWithFormat(bytes, fileSizeFormat)
}
