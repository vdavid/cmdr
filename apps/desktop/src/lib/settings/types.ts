/**
 * Settings system type definitions.
 */

import type { MessageKey } from '$lib/intl/keys.gen'
import type { IconName } from '$lib/ui/icons/icon-map'

// ============================================================================
// Core Types
// ============================================================================

export type SettingType = 'boolean' | 'number' | 'string' | 'enum' | 'duration' | 'string-array'

export type DurationUnit = 'ms' | 's' | 'min' | 'h' | 'd'

export interface EnumOption {
  value: string | number
  label: string
  description?: string
  /** Optional Lucide glyph rendered before the label (toggle-group options only). */
  icon?: IconName
}

// ============================================================================
// Registry authoring shape (carries i18n message KEYS)
//
// The registry's single source of truth stores message KEYS, not English. The
// runtime resolves them through `t()` at read time (see `settings-registry.ts`
// `resolveDefinition`), so every `getSettingDefinition(...).label` consumer keeps
// receiving a rendered string with no call-site change. Keys are typed
// `MessageKey` so a typo is a compile error and `keys.gen.ts` stays the contract.
// ============================================================================

/** An enum option as authored in the registry: a value plus message keys. */
export interface EnumOptionSource {
  value: string | number
  labelKey: MessageKey
  descriptionKey?: MessageKey
  /** Optional Lucide glyph rendered before the label (toggle-group options only). */
  icon?: IconName
}

/**
 * Constraints as authored in the registry. Options carry a message KEY
 * (`EnumOptionSource`) by default; a few carry a literal `label` (`EnumOption`)
 * when the text is NOT translatable copy (brand names from the provider table,
 * plain numerals), and pass through `resolveOption` unchanged.
 */
export interface SettingConstraintsSource extends Omit<SettingConstraints, 'options'> {
  options?: (EnumOptionSource | EnumOption)[]
}

/**
 * A setting as authored in the registry: identity, behavior, and message KEYS
 * for everything user-facing. `resolveDefinition` turns this into a
 * `SettingDefinition` whose `label`/`description`/option labels are resolved
 * (getter-backed) strings.
 */
export interface SettingDefinitionSource extends Omit<
  SettingDefinition,
  'label' | 'description' | 'constraints' | 'card'
> {
  labelKey: MessageKey
  /** Omitted for settings with no description (rendered as an empty string). */
  descriptionKey?: MessageKey
  /** i18n KEY for the in-page SectionCard group title this setting belongs to. Resolved to `card`. */
  cardKey?: MessageKey
  constraints?: SettingConstraintsSource
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
  // Typed as SettingId (not string) so a registry entry whose id is missing from
  // SettingsValues fails to compile here, right at the entry. Closes the drift where
  // a registered key absent from the SettingsValues interface forced an `as any` at
  // every getSetting/setSetting call site.
  id: SettingId
  section: string[]

  // Display
  label: string
  description: string
  keywords: string[]
  /**
   * Resolved in-page SectionCard group title this setting belongs to (from `cardKey`).
   * Descriptive/searchable only: it feeds the search index so a card title is findable,
   * and documents card membership. It is NEVER read to decide whether a card renders —
   * card visibility is owned by the section (computed from the same `shouldShow` as the
   * rows). Distinct from `section` (the nav tree / routing identity).
   */
  card?: string

  // Type and constraints
  type: SettingType
  default: unknown
  constraints?: SettingConstraints

  // Behavior
  requiresRestart?: boolean
  disabled?: boolean
  disabledReason?: string
  /** Internal state that should not appear in any section (main tree or Advanced). Persisted via the same store. */
  hidden?: boolean

  // UI hints
  component?:
    | 'switch'
    | 'checkbox'
    | 'select'
    | 'radio'
    | 'slider'
    | 'toggle-group'
    | 'number-input'
    | 'text-input'
    | 'password-input'
    | 'duration'
}

// ============================================================================
// Setting Value Types (for type-safe access)
// ============================================================================

export type UiDensity = 'compact' | 'comfortable' | 'spacious'
export type FileSizeFormat = 'binary' | 'si'
/** How file sizes are displayed: `dynamic` picks the friendliest unit per file; the others force a fixed unit; `bytes` shows raw byte triads. */
export type FileSizeUnit = 'dynamic' | 'bytes' | 'kB' | 'MB' | 'GB'
export type DateTimeFormat = 'system' | 'iso' | 'short' | 'custom'
export type NetworkTimeoutMode = 'normal' | 'slow' | 'custom'
export type ThemeMode = 'light' | 'dark' | 'system'
export type ExtensionChangePolicy = 'yes' | 'no' | 'ask'
/** What ⌘V does in a pane when the clipboard holds no file URLs but has pasteable content (text, image, PDF). */
export type PasteClipboardAsFileMode = 'doNothing' | 'createFile' | 'createFileAndRename'
export type DirectorySortMode = 'likeFiles' | 'alwaysByName'
export type SizeDisplayMode = 'smart' | 'logical' | 'physical'
export type BriefColumnWidthMode = 'paneWidth' | 'limited'
export type AppColor = 'system' | 'cmdr-gold'
export type SizeColorsPalette = 'none' | 'app' | 'rainbow'
export type DateColorsPalette = 'none' | 'app' | 'wilting'
export type VolumeTintColor =
  | 'none'
  | 'red'
  | 'orange'
  | 'amber'
  | 'lime'
  | 'green'
  | 'teal'
  | 'cyan'
  | 'blue'
  | 'indigo'
  | 'purple'
  | 'pink'
  | 'brown'
/** Ordered list of selectable tints (excludes 'none'), as shown in the picker. */
export const VOLUME_TINT_COLORS: readonly Exclude<VolumeTintColor, 'none'>[] = [
  'red',
  'orange',
  'amber',
  'lime',
  'green',
  'teal',
  'cyan',
  'blue',
  'indigo',
  'purple',
  'pink',
  'brown',
] as const
/**
 * Which folders image indexing may cover: only the ones the user chose, or those plus every
 * folder above the importance threshold. Mirrors the backend `media_index::gate::IndexScope`
 * tokens.
 */
export type MediaIndexScope = 'chosen' | 'importance'

export type DownloadsNotificationsMode = 'in-app' | 'macos' | 'both' | 'neither'
export type LowDiskSpaceNotificationsMode = 'in-app' | 'macos' | 'off'

export type AiProvider = 'off' | 'cloud' | 'local'
export type AiLocalContextSize = '2048' | '4096' | '8192' | '16384' | '32768' | '65536' | '131072' | '262144'

/**
 * UI language: `'system'` follows the OS locale (the default); any other value
 * is a BCP-47 locale tag with a loaded catalog (e.g. `'en'`, `'en-XA'`). The
 * valid non-`'system'` values are derived at runtime from the loaded catalogs
 * (`availableLocales()`), so they aren't a fixed union here. `string & {}` keeps
 * the `'system'` literal visible in autocomplete without it being swallowed by
 * the open `string` (which `'system' | string` would be, flagged as redundant).
 */
export type LanguageSetting = 'system' | (string & {})

export interface SettingsValues {
  // Appearance
  'appearance.language': LanguageSetting
  'appearance.appColor': AppColor
  'appearance.textSize': number
  'appearance.uiDensity': UiDensity
  'appearance.useAppIconsForDocuments': boolean
  'appearance.showFunctionKeyBar': boolean
  'appearance.fileSizeFormat': FileSizeFormat
  'appearance.sizeColors': SizeColorsPalette
  'appearance.dateColors': DateColorsPalette
  'appearance.dateTimeFormat': DateTimeFormat
  'appearance.customDateTimeFormat': string
  'appearance.tintLocal': VolumeTintColor
  'appearance.tintSmb': VolumeTintColor
  'appearance.tintMtp': VolumeTintColor

  // Listing
  'listing.directorySortMode': DirectorySortMode
  'listing.sizeDisplay': SizeDisplayMode
  'listing.sizeUnit': FileSizeUnit
  'listing.sizeMismatchWarning': boolean
  'listing.stripedRows': boolean
  'listing.showExtensionInName': boolean
  'listing.showTags': boolean
  'listing.briefColumnWidthMode': BriefColumnWidthMode
  'listing.briefColumnWidthMaxPx': number

  // Git
  'fileExplorer.git.showRepoChip': boolean
  'fileExplorer.git.showStatusColumn': boolean
  'fileExplorer.git.showVirtualGitPortal': boolean

  // Type-to-jump
  'fileExplorer.typeToJump.resetDelay': number

  // Quick Look
  'fileExplorer.suppressQuickLookHint': boolean

  // Navigation
  'behavior.doubleClickPaneNavigatesToParent': boolean
  'behavior.doubleClickOnPaneNotificationSeen': boolean

  // Archives (Enter behavior per format: pinned-shape JSON, `{ zip: 'ask', … }`)
  'behavior.archiveEnterBehavior': string
  // Deflate level (1..=9, default 6) for user-driven zip writes; read at dispatch and passed in the operation config
  'behavior.archiveCompressionLevel': number

  // File operations
  'fileOperations.mtpEnabled': boolean
  'fileOperations.mtpConnectionWarning': boolean
  'fileOperations.allowFileExtensionChanges': ExtensionChangePolicy
  'fileOperations.pasteClipboardAsFile': PasteClipboardAsFileMode
  'fileOperations.progressUpdateInterval': number
  'fileOperations.maxConflictsToShow': number

  // Updates & privacy
  'updates.autoCheck': boolean
  'updates.crashReports': boolean
  'updates.errorReports': boolean
  /** Sticky default for the report dialogs' attach-email checkbox; also a manual toggle in Advanced. */
  'updates.attachEmailToReports': boolean
  /** Show the "What's new" popup after Cmdr updates itself. */
  'whatsNew.showOnUpdate': boolean
  /** Hidden: the version we last showed the user in the "What's new" popup (`''` = never stamped). */
  'whatsNew.lastSeenVersion': string

  // Analytics (beta usage stats + optional contact email)
  'analytics.enabled': boolean
  'analytics.email': string

  // Network
  'network.enabled': boolean
  'network.firstTriggerDone': boolean
  'network.directSmbConnection': boolean
  'network.shareCacheDuration': number
  'network.timeoutMode': NetworkTimeoutMode
  'network.customTimeout': number
  'network.smbConcurrency': number

  // Theme
  'theme.mode': ThemeMode

  // Indexing
  'indexing.enabled': boolean
  /**
   * Hidden search anchor for the "Index size / Clear index" action row, which is
   * hand-rendered (not a real control). Never read or written; exists only so the
   * row is searchable ("index size") and its card can show. See settings-registry.ts.
   */
  'indexing.indexSize': boolean
  /** Gates the per-drive first-connect "turn on indexing?" notification (D6). On by default. */
  'indexing.askForEachDrive': boolean
  /** Gates the one-time "your drive went stale" dialog (D2). The yellow badge shows regardless. On by default. */
  'indexing.staleNotify': boolean
  /**
   * Internal (FE-owned): JSON array of volume ids the user silenced via "Don''t ask
   * again for this drive". The first-connect notification skips a silenced drive.
   * Cleared by the "Re-enable notifications for all drives" settings button.
   */
  'indexing.silencedDrives': string
  /** Internal (FE-owned): whether the one-time stale dialog (D2) has fired once. */
  'indexing.firstStaleDialogShown': boolean

  // Image-ML index (media_index): make images searchable by the text inside them.
  /**
   * Master toggle for image-content indexing (OCR search, off by default). Live-applied
   * to the backend `media_index` scheduler via `set_image_index_enabled`; the scheduler
   * no-ops until it's on. Local drives only for now (SMB/MTP is a later milestone).
   */
  'mediaIndex.enabled': boolean
  /**
   * Whether the file list draws a small per-file image-index status badge (indexed /
   * pending / stale / excluded / couldn't-index). FE-only render toggle (default on); when
   * off, the overlay is neither fetched nor drawn. Gated together with `mediaIndex.enabled`.
   */
  'mediaIndex.showFileStatusIcons': boolean
  /**
   * Internal (FE-owned): volume ids opted into background network (SMB) image enrichment
   * (`media_index` network enrichment). Off by default per volume: turning on the master toggle does NOT
   * auto-enrich network drives. Persisted as a real JSON array so the Rust loader reads it
   * as `Vec<String>`; seeded into `media_index::network::config` at startup and live-applied
   * via `media_index_set_network_volume_enabled` on change.
   */
  'mediaIndex.networkVolumes': string[]
  /**
   * Internal (FE-owned): volume ids marked "always index" (enrich regardless of the
   * importance threshold, so a rarely-browsed NAS archive's photos don't defer forever).
   * Persisted as a real JSON array; live-applied via `media_index_set_always_index_volume`.
   */
  'mediaIndex.alwaysIndexVolumes': string[]
  /**
   * Internal (FE-owned): absolute OS-mount folder paths marked "always index"; every image
   * at or under one enriches regardless of importance. Persisted as a real JSON array;
   * live-applied via `media_index_set_always_index_folder`.
   */
  'mediaIndex.alwaysIndexFolders': string[]
  /**
   * Internal (FE-owned): absolute OS folder paths EXCLUDED from image indexing (the privacy
   * veto — no image at or under one is enriched, beating any "always index" override). Set
   * by the folder context-menu "Don't index images in this folder" item. Persisted as a real
   * JSON array; live-applied via `media_index_set_excluded_folder`, which also retro-deletes
   * the folder's already-indexed rows.
   */
  'mediaIndex.excludedFolders': string[]
  /**
   * The lowest folder-importance level (`0.0..=1.0`) the image indexer enriches — the
   * "how deep do I index?" slider's typed value. `0.0` (the default, matching the backend
   * `DEFAULT_IMPORTANCE_THRESHOLD`) enriches every scored folder (junk like `node_modules`
   * is floored out regardless); raising it defers low-importance folders so a huge library
   * indexes the folders you actually use first. Live-applied via
   * `media_index_set_importance_threshold`; the scheduler reads the same signal, so the
   * control and the behavior can't drift. Rendered as named buckets in the "Image search"
   * card (`MediaIndexImportanceSlider.svelte`), not an auto row, so it's `hidden`.
   */
  'mediaIndex.importanceThreshold': number
  /**
   * How many parallel workers image indexing runs (the "Parallel workers" slider). `1`
   * (the default) is today's single worker; the max is this machine's CPU count, and the
   * backend clamps to `1..=CPU-count`. The M2 spike measured a ~1.25x ceiling on current
   * Apple Silicon (the ANE serializes inference), so more workers help modestly and only up
   * to ~2. Live-applied via `media_index_set_parallelism`; a running pass resizes its pool
   * between images. Hand-rendered via `SettingSlider` with a runtime max, so it's `hidden`.
   */
  'mediaIndex.parallelism': number
  /**
   * WHICH folders image indexing may cover. `'chosen'` (the default) indexes only the folders
   * and drives the user named — the `mediaIndex.alwaysIndexFolders` / `alwaysIndexVolumes`
   * overrides — and never consults folder importance, so `mediaIndex.importanceThreshold` has
   * no effect at all. `'importance'` adds every folder at or above that threshold, and is the
   * only value where the slider is shown. Live-applied via `media_index_set_scope`; broadening
   * kicks a pass, narrowing deletes nothing (the now-uncovered rows stay searchable and surface
   * as the reclaim offer). Rendered by `MediaIndexScope.svelte`, not an auto row, so it's
   * `hidden`.
   */
  'mediaIndex.scope': MediaIndexScope
  /**
   * Whether CLIP semantic search ("search photos by description") is on. Default `true`, so
   * once the on-device model is installed a covered image becomes findable by describing it.
   * One backend atomic gates both sides: `search_semantic` returns `[]` when off, and no
   * enrichment pass embeds CLIP when off (so turning it off stops new CLIP work without
   * deleting anything). Live-applied via `media_index_set_semantic_search_enabled`. Rendered
   * as a `SettingSwitch` in the "Semantic search" card, not an auto row, so it's `hidden`.
   */
  'mediaIndex.semanticSearch.enabled': boolean

  // Behavior › Notifications - downloads notifications + global go-to-latest shortcut.
  // (The `fileSystemWatching` id prefix is a stable persistence key, kept across the
  // section rename from "File system watching" to "Notifications".)
  'behavior.fileSystemWatching.downloadsNotifications': DownloadsNotificationsMode
  /** Internal: remembers whether the user last collapsed the new-download toast, so a new toast opens the same way. */
  'behavior.fileSystemWatching.downloadsToastCollapsed': boolean
  'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled': boolean
  'behavior.fileSystemWatching.globalGoToLatestShortcut.binding': string
  /** Internal: suppresses the first-trigger warn toast once the user acknowledges it. */
  'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged': boolean
  'behavior.fileSystemWatching.lowDiskSpaceNotifications': LowDiskSpaceNotificationsMode
  'behavior.fileSystemWatching.lowDiskSpaceThresholdPercent': number

  // Operation log (retention). Read by the Rust retention loop each tick
  // (`settings::load_operation_log_retention_limits`), so a change takes effect on
  // the next prune with no restart. Age is a duration in ms (`0` = the "Forever"
  // sentinel ⇒ never prune by age); size is a byte budget (default 3 GB).
  'operationLog.maxAge': number
  'operationLog.maxSize': number

  // Viewer
  'viewer.wordWrap': boolean
  'fileViewer.suppressBinaryWarning': boolean

  // AI
  'ai.provider': AiProvider
  'ai.localContextSize': AiLocalContextSize
  'ai.cloudProvider': string
  'ai.cloudProviderConfigs': string // JSON blob

  // Ask Cmdr
  // The interactive-slot model override (empty = use the shared `ai/` provider's model).
  // Read fresh backend-side each send (`load_ask_cmdr_interactive_model`); a later bulk
  // slot slots in as its own additive key with no migration (agent-spec D43).
  'askCmdr.interactiveModel': string

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
  'advanced.filterSafeSaveArtifacts': boolean
  'advanced.logLlmCalls': boolean
  'advanced.diskSpaceChangeThreshold': number
  'advanced.maxLogStorageMb': number
  'fileExplorer.tabs.closedTabHistorySize': number

  // Search
  'search.autoApply': boolean
  'search.recentSearches.maxCount': number

  // Selection
  'selection.recentSelections.maxCount': number

  // Onboarding (internal state, hidden from UI)
  'onboarding.upgradeNudgeShown': boolean
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

/** Milliseconds per `DurationUnit`. The one source of truth for unit scaling; UI that
    edits a `duration` setting in its display unit multiplies/divides through this. */
export const DURATION_UNIT_MS: Record<DurationUnit, number> = {
  ms: 1,
  s: 1000,
  min: 60_000,
  h: 3_600_000,
  d: 86_400_000,
}

/** ms-per-unit for `unit`, or 1 when `unit` is absent (a plain, unscaled number). */
export function durationUnitFactor(unit: DurationUnit | undefined): number {
  return unit ? DURATION_UNIT_MS[unit] : 1
}

/** Stored milliseconds → the value shown in the `unit` field (e.g. 20000 ms, `'s'` → 20). */
export function msToDurationValue(ms: number, unit: DurationUnit | undefined): number {
  return ms / durationUnitFactor(unit)
}

/** A `unit`-field value → stored milliseconds (e.g. 20, `'s'` → 20000 ms). */
export function durationValueToMs(value: number, unit: DurationUnit | undefined): number {
  return value * durationUnitFactor(unit)
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return ms.toString() + 'ms'
  if (ms < 60000) return (ms / 1000).toString() + 's'
  if (ms < 3600000) return (ms / 60000).toString() + 'min'
  if (ms < 86400000) return (ms / 3600000).toString() + 'h'
  return (ms / 86400000).toString() + 'd'
}
