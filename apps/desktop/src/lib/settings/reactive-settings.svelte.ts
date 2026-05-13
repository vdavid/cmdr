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
  type BriefColumnWidthMode,
  type AppColor,
  densityMappings,
} from '$lib/settings'
import { formatDateForDisplay, formatFileSizeWithFormat, type FormattedDate } from './format-utils'
import { getAppLogger } from '$lib/logging/logger'
import { clearExtensionIconCache } from '$lib/icon-cache'
import { getEffectiveScale } from '$lib/text-size.svelte'

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
let humanFriendlySizeUnits = $state<boolean>(true)
let sizeMismatchWarning = $state<boolean>(true)
let stripedRows = $state<boolean>(false)
let briefColumnWidthMode = $state<BriefColumnWidthMode>('paneWidth')
let briefColumnWidthMaxPx = $state<number>(400)
let networkEnabled = $state<boolean>(true)
let typeToJumpResetDelay = $state<number>(1000)

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
    humanFriendlySizeUnits = getSetting('listing.humanFriendlySizeUnits')
    sizeMismatchWarning = getSetting('listing.sizeMismatchWarning')
    stripedRows = getSetting('listing.stripedRows')
    briefColumnWidthMode = getSetting('listing.briefColumnWidthMode')
    briefColumnWidthMaxPx = getSetting('listing.briefColumnWidthMaxPx')
    networkEnabled = getSetting('network.enabled')
    typeToJumpResetDelay = getSetting('fileExplorer.typeToJump.resetDelay')

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
        case 'listing.humanFriendlySizeUnits':
          humanFriendlySizeUnits = value as boolean
          break
        case 'listing.sizeMismatchWarning':
          sizeMismatchWarning = value as boolean
          break
        case 'listing.stripedRows':
          stripedRows = value as boolean
          break
        case 'listing.briefColumnWidthMode':
          briefColumnWidthMode = value as BriefColumnWidthMode
          break
        case 'listing.briefColumnWidthMaxPx':
          briefColumnWidthMaxPx = value as number
          break
        case 'network.enabled':
          networkEnabled = value as boolean
          break
        case 'fileExplorer.typeToJump.resetDelay':
          typeToJumpResetDelay = value as number
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

/**
 * Get current row height in pixels.
 *
 * Compounds the density baseline with the effective text scale (system
 * Accessibility × user `appearance.textSize`). Reading `getEffectiveScale()`
 * inside this function makes it trackable inside `$derived`/`$effect`, so
 * components that do `const rowHeight = $derived(getRowHeight())` re-flow
 * automatically when the user moves the text-size slider.
 */
export function getRowHeight(): number {
  return Math.round(densityMappings[uiDensity].rowHeight * getEffectiveScale())
}

/**
 * Get current icon size in pixels (file-list icons, etc.).
 *
 * Density is intentionally NOT a factor — the historical icon size was a
 * hardcoded 16 px regardless of density. Only the text-size scale applies.
 * Components that need this in JS (e.g. for `grid-template-columns`) read
 * this getter; the matching CSS token is `--spacing-icon-size` in app.css.
 */
const ICON_SIZE_BASE = 16
export function getIconSize(): number {
  return Math.round(ICON_SIZE_BASE * getEffectiveScale())
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

/** Get whether to show human-friendly size units (e.g. "1.02 MB") instead of raw bytes */
export function getHumanFriendlySizeUnits(): boolean {
  return humanFriendlySizeUnits
}

/** Get current file size format ("binary" or "si") */
export function getFileSizeFormat(): FileSizeFormat {
  return fileSizeFormat
}

/** Get whether striped rows are enabled */
export function getStripedRows(): boolean {
  return stripedRows
}

/** Get the Brief mode column-width mode: 'paneWidth' lets columns fill the pane; 'limited' caps them at `getBriefColumnWidthMaxPx`. */
export function getBriefColumnWidthMode(): BriefColumnWidthMode {
  return briefColumnWidthMode
}

/** Get the Brief mode column-width pixel limit (applies only when mode is 'limited'). */
export function getBriefColumnWidthMaxPx(): number {
  return briefColumnWidthMaxPx
}

/** Get whether networking (SMB discovery + connections) is enabled. */
export function getNetworkEnabled(): boolean {
  return networkEnabled
}

/**
 * Get the type-to-jump buffer reset delay in milliseconds.
 *
 * The factory in `file-explorer/pane/type-to-jump-state.svelte.ts` reads this
 * via its `getResetMs` callback on every keystroke, so changes in the Advanced
 * settings section take effect on the next keystroke without app restart.
 */
export function getTypeToJumpResetDelay(): number {
  return typeToJumpResetDelay
}

// ============================================================================
// Formatting utilities that use reactive settings
// ============================================================================

/**
 * The single reactive entry point for everything the UI shows about a date.
 * Returns the joined `text`, structured `parts` (with the year extracted for
 * coloring), and the `ageClass`. New date-touching components should call
 * this rather than reaching for `Date#toLocaleString` or building their own
 * formatters — keep date display consistent across the app.
 *
 * For the plain string form, use the `formatDateTime` shortcut below.
 *
 * @param timestamp Unix timestamp in seconds
 */
export function formattedDate(timestamp: number | null | undefined): FormattedDate {
  return formatDateForDisplay(timestamp, dateTimeFormat, customDateTimeFormat)
}

/**
 * Shortcut for `formattedDate(ts).text` — the joined plain string. Use for
 * tooltips, MCP responses, clipboard copies, and anywhere a one-line label is
 * wanted. UI rendering should prefer the `<DateLabel>` component or destructure
 * `formattedDate(ts).parts` directly.
 */
export function formatDateTime(timestamp: number | null | undefined): string {
  return formattedDate(timestamp).text
}

/**
 * Format bytes as human-readable string according to current settings.
 * @param bytes Number of bytes
 */
export function formatFileSize(bytes: number): string {
  return formatFileSizeWithFormat(bytes, fileSizeFormat)
}
