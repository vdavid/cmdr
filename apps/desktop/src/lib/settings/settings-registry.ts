/**
 * Settings registry - single source of truth for all settings.
 *
 * The registry stores message KEYS for everything user-facing (label,
 * description, enum-option labels), not English. `resolveDefinition` turns each
 * authored `SettingDefinitionSource` into a `SettingDefinition` whose `label` /
 * `description` / option labels are getter-backed: reading them resolves the
 * current catalog string through `t()` (`$lib/intl`). This keeps the whole
 * `getSettingDefinition(...).label` consumer surface unchanged while making the
 * copy translation-ready and single-sourced in `messages/en/settings.json`.
 * Section identity (`section: string[]`) stays English on purpose — it's a
 * structural key for routing, the section tree, and search, not a render path;
 * the rendered section TITLES live in the section components via `t()`.
 */

import type {
  EnumOption,
  EnumOptionSource,
  SettingConstraints,
  SettingConstraintsSource,
  SettingDefinition,
  SettingDefinitionSource,
  SettingId,
  SettingsValues,
} from './types'
import { SettingValidationError, VOLUME_TINT_COLORS } from './types'
import { cloudProviderPresets } from './cloud-providers'
import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Message-key leaf for each tint color, used by the tint enum options. */
const TINT_COLOR_KEY: Record<string, MessageKey> = {
  none: 'settings.tint.none',
  red: 'settings.tint.red',
  orange: 'settings.tint.orange',
  amber: 'settings.tint.amber',
  lime: 'settings.tint.lime',
  green: 'settings.tint.green',
  teal: 'settings.tint.teal',
  cyan: 'settings.tint.cyan',
  blue: 'settings.tint.blue',
  indigo: 'settings.tint.indigo',
  purple: 'settings.tint.purple',
  pink: 'settings.tint.pink',
  brown: 'settings.tint.brown',
}

/** Options list for the three `appearance.tint{Local,Smb,Mtp}` settings. */
const TINT_COLOR_OPTIONS: EnumOptionSource[] = [
  { value: 'none', labelKey: TINT_COLOR_KEY.none },
  ...VOLUME_TINT_COLORS.map((c) => ({ value: c, labelKey: TINT_COLOR_KEY[c] })),
]

// ============================================================================
// Settings Definitions
//
// Top-level section order is driven by the order entries appear here.
// `buildSectionTree()` uses first-appearance order for each (sub)section name.
// Special non-registry sections (Keyboard shortcuts, License, Advanced) are
// interleaved in `SettingsSidebar.svelte`.
// ============================================================================

const settingsRegistrySource: SettingDefinitionSource[] = [
  // ========================================================================
  // Appearance › Colors and formats
  // ========================================================================
  {
    id: 'theme.mode',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.theme.mode.label',
    descriptionKey: 'settings.theme.mode.description',
    keywords: ['theme', 'dark', 'light', 'mode', 'appearance', 'color'],
    type: 'enum',
    default: 'system',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'light', labelKey: 'settings.theme.mode.opt.light' },
        { value: 'dark', labelKey: 'settings.theme.mode.opt.dark' },
        { value: 'system', labelKey: 'settings.theme.mode.opt.system' },
      ],
    },
  },
  {
    id: 'appearance.appColor',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.appColor.label',
    // The rendered description is a `<LinkButton>` snippet in `AppearanceSection`
    // (platform-branching via `isMacOS()`); this key feeds search only.
    descriptionKey: 'settings.appearance.appColor.description',
    keywords: ['color', 'accent', 'theme', 'gold', 'system', 'brand'],
    type: 'enum',
    default: 'system',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', labelKey: 'settings.appearance.appColor.opt.system' },
        { value: 'cmdr-gold', labelKey: 'settings.appearance.appColor.opt.cmdrGold' },
      ],
    },
  },
  {
    id: 'appearance.sizeColors',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.sizeColors.label',
    descriptionKey: 'settings.appearance.sizeColors.description',
    keywords: ['size', 'color', 'tier', 'rainbow', 'app', 'highlight', 'kb', 'mb', 'gb', 'tb'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', labelKey: 'settings.appearance.sizeColors.opt.none' },
        { value: 'app', labelKey: 'settings.appearance.sizeColors.opt.app' },
        { value: 'rainbow', labelKey: 'settings.appearance.sizeColors.opt.rainbow' },
      ],
    },
  },
  {
    id: 'appearance.dateColors',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.dateColors.label',
    descriptionKey: 'settings.appearance.dateColors.description',
    keywords: ['date', 'color', 'age', 'modified', 'wilting', 'app', 'fresh', 'old'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', labelKey: 'settings.appearance.dateColors.opt.none' },
        { value: 'app', labelKey: 'settings.appearance.dateColors.opt.app' },
        { value: 'wilting', labelKey: 'settings.appearance.dateColors.opt.wilting' },
      ],
    },
  },
  {
    id: 'appearance.dateTimeFormat',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.dateTimeFormat.label',
    descriptionKey: 'settings.appearance.dateTimeFormat.description',
    keywords: ['date', 'time', 'format', 'iso', 'custom', 'timestamp'],
    type: 'enum',
    default: 'iso',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', labelKey: 'settings.appearance.dateTimeFormat.opt.system' },
        {
          value: 'iso',
          labelKey: 'settings.appearance.dateTimeFormat.opt.iso',
          descriptionKey: 'settings.appearance.dateTimeFormat.optDesc.iso',
        },
        {
          value: 'short',
          labelKey: 'settings.appearance.dateTimeFormat.opt.short',
          descriptionKey: 'settings.appearance.dateTimeFormat.optDesc.short',
        },
        { value: 'custom', labelKey: 'settings.appearance.dateTimeFormat.opt.custom' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'appearance.customDateTimeFormat',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.customDateTimeFormat.label',
    descriptionKey: 'settings.appearance.customDateTimeFormat.description',
    keywords: ['custom', 'format', 'date', 'time', 'placeholder'],
    type: 'string',
    default: 'YYYY-MM-DD HH:mm',
    component: 'text-input',
  },
  {
    id: 'listing.stripedRows',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.listing.stripedRows.label',
    descriptionKey: 'settings.listing.stripedRows.description',
    keywords: ['stripe', 'zebra', 'alternate', 'row', 'shading', 'accessibility', 'a11y'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  // Volume tints (12-color picker, rendered by `AppearanceSection.svelte` via
  // `SettingColorSwatchPicker`, not the registry-driven enum components).
  // Enum type carries the valid values for MCP agents and runtime validation;
  // live-apply happens reactively in `FilePane.svelte` via `volume-tint.svelte.ts`.
  {
    id: 'appearance.tintLocal',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintLocal.label',
    descriptionKey: 'settings.appearance.tintLocal.description',
    keywords: ['tint', 'pane', 'color', 'volume', 'local', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintSmb',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintSmb.label',
    descriptionKey: 'settings.appearance.tintSmb.description',
    keywords: ['tint', 'pane', 'color', 'volume', 'smb', 'network', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintMtp',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintMtp.label',
    descriptionKey: 'settings.appearance.tintMtp.description',
    keywords: ['tint', 'pane', 'color', 'volume', 'mtp', 'android', 'kindle', 'camera', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },

  // ========================================================================
  // Appearance › Zoom and density
  // ========================================================================
  {
    id: 'appearance.textSize',
    section: ['Appearance', 'Zoom and density'],
    labelKey: 'settings.appearance.textSize.label',
    descriptionKey: 'settings.appearance.textSize.description',
    keywords: ['text', 'size', 'font', 'larger', 'smaller', 'accessibility', 'a11y', 'zoom', 'scale'],
    type: 'number',
    default: 100,
    component: 'slider',
    constraints: {
      min: 75,
      max: 150,
      step: 5,
      sliderStops: [75, 100, 125, 150],
    },
  },
  {
    id: 'appearance.uiDensity',
    section: ['Appearance', 'Zoom and density'],
    labelKey: 'settings.appearance.uiDensity.label',
    descriptionKey: 'settings.appearance.uiDensity.description',
    keywords: ['compact', 'comfortable', 'spacious', 'size', 'spacing', 'dense'],
    type: 'enum',
    default: 'comfortable',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'compact', labelKey: 'settings.appearance.uiDensity.opt.compact' },
        { value: 'comfortable', labelKey: 'settings.appearance.uiDensity.opt.comfortable' },
        { value: 'spacious', labelKey: 'settings.appearance.uiDensity.opt.spacious' },
      ],
    },
  },

  // ========================================================================
  // Appearance › File and folder sizes
  // ========================================================================
  {
    id: 'listing.sizeDisplay',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeDisplay.label',
    descriptionKey: 'settings.listing.sizeDisplay.description',
    keywords: ['size', 'display', 'logical', 'physical', 'smart', 'disk', 'content', 'sparse'],
    type: 'enum',
    default: 'smart',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'smart', labelKey: 'settings.listing.sizeDisplay.opt.smart' },
        { value: 'logical', labelKey: 'settings.listing.sizeDisplay.opt.logical' },
        { value: 'physical', labelKey: 'settings.listing.sizeDisplay.opt.physical' },
      ],
    },
  },
  {
    id: 'listing.sizeUnit',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeUnit.label',
    descriptionKey: 'settings.listing.sizeUnit.description',
    keywords: ['size', 'human', 'bytes', 'unit', 'format', 'raw', 'precise', 'kb', 'mb', 'gb', 'dynamic'],
    type: 'enum',
    default: 'dynamic',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'dynamic', labelKey: 'settings.listing.sizeUnit.opt.dynamic' },
        { value: 'bytes', labelKey: 'settings.listing.sizeUnit.opt.bytes' },
        { value: 'kB', labelKey: 'settings.listing.sizeUnit.opt.kB' },
        { value: 'MB', labelKey: 'settings.listing.sizeUnit.opt.mB' },
        { value: 'GB', labelKey: 'settings.listing.sizeUnit.opt.gB' },
      ],
    },
  },
  {
    id: 'appearance.fileSizeFormat',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.appearance.fileSizeFormat.label',
    descriptionKey: 'settings.appearance.fileSizeFormat.description',
    keywords: ['size', 'bytes', 'binary', 'decimal', 'kb', 'mb', 'kib', 'mib'],
    type: 'enum',
    default: 'binary',
    component: 'select',
    constraints: {
      options: [
        {
          value: 'binary',
          labelKey: 'settings.appearance.fileSizeFormat.opt.binary',
          descriptionKey: 'settings.appearance.fileSizeFormat.optDesc.binary',
        },
        {
          value: 'si',
          labelKey: 'settings.appearance.fileSizeFormat.opt.si',
          descriptionKey: 'settings.appearance.fileSizeFormat.optDesc.si',
        },
      ],
    },
  },
  {
    id: 'listing.sizeMismatchWarning',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeMismatchWarning.label',
    descriptionKey: 'settings.listing.sizeMismatchWarning.description',
    keywords: ['size', 'mismatch', 'warning', 'alert', 'disk', 'content', 'difference'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Appearance › Listing
  // ========================================================================
  {
    id: 'appearance.useAppIconsForDocuments',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.appearance.useAppIconsForDocuments.label',
    descriptionKey: 'settings.appearance.useAppIconsForDocuments.description',
    keywords: ['icon', 'document', 'file', 'app', 'colorful', 'finder'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'appearance.showFunctionKeyBar',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.appearance.showFunctionKeyBar.label',
    descriptionKey: 'settings.appearance.showFunctionKeyBar.description',
    keywords: ['function', 'key', 'bar', 'f-key', 'fkey', 'shortcut', 'buttons', 'bottom', 'toolbar'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'listing.showExtensionInName',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.showExtensionInName.label',
    descriptionKey: 'settings.listing.showExtensionInName.description',
    keywords: ['extension', 'ext', 'filename', 'name', 'column', 'full', 'split', 'suffix'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'listing.directorySortMode',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.directorySortMode.label',
    descriptionKey: 'settings.listing.directorySortMode.description',
    keywords: ['sort', 'directory', 'folder', 'order', 'listing', 'name', 'size'],
    type: 'enum',
    default: 'likeFiles',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'likeFiles', labelKey: 'settings.listing.directorySortMode.opt.likeFiles' },
        { value: 'alwaysByName', labelKey: 'settings.listing.directorySortMode.opt.alwaysByName' },
      ],
    },
  },
  {
    id: 'listing.briefColumnWidthMode',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.briefColumnWidthMode.label',
    descriptionKey: 'settings.listing.briefColumnWidthMode.description',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pane', 'shrink-wrap'],
    type: 'enum',
    default: 'paneWidth',
    component: 'radio',
    constraints: {
      options: [
        { value: 'paneWidth', labelKey: 'settings.listing.briefColumnWidthMode.opt.paneWidth' },
        { value: 'limited', labelKey: 'settings.listing.briefColumnWidthMode.opt.limited' },
      ],
    },
  },
  {
    id: 'listing.briefColumnWidthMaxPx',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.briefColumnWidthMaxPx.label',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pixel', 'slider'],
    type: 'number',
    default: 400,
    component: 'slider',
    constraints: {
      min: 250,
      max: 1000,
      step: 25,
      sliderStops: [250, 400, 600, 800, 1000],
    },
  },

  // ========================================================================
  // Behavior › File operations
  // ========================================================================
  {
    id: 'fileOperations.allowFileExtensionChanges',
    section: ['Behavior', 'File operations'],
    labelKey: 'settings.fileOperations.allowFileExtensionChanges.label',
    descriptionKey: 'settings.fileOperations.allowFileExtensionChanges.description',
    keywords: ['extension', 'rename', 'file', 'change', 'ask', 'confirm'],
    type: 'enum',
    default: 'ask',
    component: 'radio',
    constraints: {
      options: [
        { value: 'yes', labelKey: 'settings.fileOperations.allowFileExtensionChanges.opt.yes' },
        { value: 'no', labelKey: 'settings.fileOperations.allowFileExtensionChanges.opt.no' },
        { value: 'ask', labelKey: 'settings.fileOperations.allowFileExtensionChanges.opt.ask' },
      ],
    },
  },

  // ========================================================================
  // Behavior › File system watching
  // (formerly "Drive indexing"; renamed so the indexer and the downloads
  // watcher both live under one umbrella that shares the FDA gate)
  // ========================================================================
  {
    id: 'indexing.enabled',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.enabled.label',
    descriptionKey: 'settings.indexing.enabled.description',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'behavior.fileSystemWatching.downloadsNotifications',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.description',
    keywords: ['download', 'downloads', 'notification', 'toast', 'notify', 'macos'],
    type: 'enum',
    default: 'in-app',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'in-app', labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.opt.inApp' },
        { value: 'macos', labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.opt.macos' },
        { value: 'both', labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.opt.both' },
        { value: 'neither', labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.opt.neither' },
      ],
    },
  },
  {
    id: 'behavior.fileSystemWatching.globalGoToLatestShortcut.enabled',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description',
    keywords: ['shortcut', 'hotkey', 'global', 'download', 'downloads', 'jump', 'go to', 'goto'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // The combo is edited in `Keyboard shortcuts` (see
    // `lib/downloads/GlobalShortcutRow.svelte`), not as a row here. Kept in the
    // registry because it's persisted in settings.json and the Rust
    // startup/focus refresh reads it from there before any window loads.
    // Hidden so it doesn't surface as an orphan row in search / Advanced.
    id: 'behavior.fileSystemWatching.globalGoToLatestShortcut.binding',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.binding.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.binding.description',
    keywords: ['shortcut', 'hotkey', 'global', 'binding', 'combo'],
    type: 'string',
    default: '\u{2303}\u{2325}\u{2318}J', // ⌃⌥⌘J
    component: 'text-input',
    hidden: true,
  },
  {
    // Internal: hidden from the Settings UI. Drives the first-trigger warn-toast
    // suppression. Reset on `binding` change via `setGlobalGoToLatestBinding`.
    id: 'behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.acknowledged.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal: hidden from the Settings UI. Remembers whether the user last
    // collapsed the "new download" toast, so a new toast opens in the same
    // state. Driven entirely by the toast's collapse/expand button.
    id: 'behavior.fileSystemWatching.downloadsToastCollapsed',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.downloadsToastCollapsed.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.downloadsToastCollapsed.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'behavior.fileSystemWatching.lowDiskSpaceNotifications',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.description',
    keywords: ['disk', 'space', 'low', 'free', 'storage', 'full', 'warning', 'notification', 'boot', 'startup'],
    type: 'enum',
    default: 'in-app',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'in-app', labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.opt.inApp' },
        { value: 'macos', labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.opt.macos' },
        { value: 'off', labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.opt.off' },
      ],
    },
  },
  {
    id: 'behavior.fileSystemWatching.lowDiskSpaceThresholdPercent',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceThresholdPercent.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceThresholdPercent.description',
    keywords: ['disk', 'space', 'threshold', 'percent', 'low', 'free', 'warning'],
    type: 'number',
    default: 5,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },

  // ========================================================================
  // Behavior › Search
  // ========================================================================
  {
    id: 'search.autoApply',
    section: ['Behavior', 'Search'],
    labelKey: 'settings.search.autoApply.label',
    descriptionKey: 'settings.search.autoApply.description',
    keywords: ['search', 'auto', 'apply', 'live', 'debounce', 'filename', 'regex', 'instant'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // AI
  // ========================================================================
  {
    id: 'ai.provider',
    section: ['AI'],
    labelKey: 'settings.ai.provider.label',
    descriptionKey: 'settings.ai.provider.description',
    keywords: ['ai', 'provider', 'cloud', 'openai', 'anthropic', 'claude', 'gemini', 'local', 'llm', 'off', 'model'],
    type: 'enum',
    default: 'off',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'off', labelKey: 'settings.ai.provider.opt.off' },
        { value: 'cloud', labelKey: 'settings.ai.provider.opt.cloud' },
        { value: 'local', labelKey: 'settings.ai.provider.opt.local' },
      ],
    },
  },
  {
    id: 'ai.cloudProvider',
    section: ['AI'],
    labelKey: 'settings.ai.cloudProvider.label',
    descriptionKey: 'settings.ai.cloudProvider.description',
    keywords: [
      'cloud',
      'provider',
      'service',
      'openai',
      'anthropic',
      'groq',
      'together',
      'fireworks',
      'mistral',
      'ollama',
      'deepseek',
      'xai',
      'perplexity',
      'openrouter',
      'gemini',
      'azure',
      'lm-studio',
      'custom',
    ],
    type: 'enum',
    default: 'openai',
    component: 'select',
    constraints: {
      // Cloud-provider option labels are brand names (not translatable copy),
      // sourced from the provider preset table, not the catalog.
      options: cloudProviderPresets.map((p) => ({ value: p.id, label: p.name })),
    },
  },
  {
    id: 'ai.cloudProviderConfigs',
    section: ['AI'],
    labelKey: 'settings.ai.cloudProviderConfigs.label',
    descriptionKey: 'settings.ai.cloudProviderConfigs.description',
    keywords: [],
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    id: 'ai.localContextSize',
    section: ['AI'],
    labelKey: 'settings.ai.localContextSize.label',
    descriptionKey: 'settings.ai.localContextSize.description',
    keywords: ['context', 'window', 'tokens', 'memory', 'size', 'local'],
    type: 'enum',
    default: '4096',
    component: 'select',
    constraints: {
      // Token-count option labels are plain numerals, not translatable copy.
      options: [
        { value: '2048', label: '2048' },
        { value: '4096', label: '4096' },
        { value: '8192', label: '8192' },
        { value: '16384', label: '16384' },
        { value: '32768', label: '32768' },
        { value: '65536', label: '65536' },
        { value: '131072', label: '131072' },
        { value: '262144', label: '262144' },
      ],
    },
  },

  // ========================================================================
  // File systems › SMB/Network shares
  // ========================================================================
  {
    id: 'network.enabled',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.enabled.label',
    descriptionKey: 'settings.network.enabled.description',
    keywords: ['network', 'enable', 'enabled', 'smb', 'discovery', 'mdns', 'bonjour', 'local', 'permission'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.firstTriggerDone',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.firstTriggerDone.label',
    descriptionKey: 'settings.network.firstTriggerDone.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'network.directSmbConnection',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.directSmbConnection.label',
    descriptionKey: 'settings.network.directSmbConnection.description',
    keywords: ['smb', 'direct', 'fast', 'connection', 'network', 'performance', 'smb2'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.shareCacheDuration',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.shareCacheDuration.label',
    descriptionKey: 'settings.network.shareCacheDuration.description',
    keywords: ['cache', 'smb', 'share', 'network', 'refresh', 'ttl'],
    type: 'duration',
    default: 30000, // 30 seconds in ms
    component: 'select',
    constraints: {
      unit: 's',
      options: [
        { value: 30000, labelKey: 'settings.network.shareCacheDuration.opt.s30' },
        { value: 300000, labelKey: 'settings.network.shareCacheDuration.opt.min5' },
        { value: 3600000, labelKey: 'settings.network.shareCacheDuration.opt.hour1' },
        { value: 86400000, labelKey: 'settings.network.shareCacheDuration.opt.day1' },
        { value: 2592000000, labelKey: 'settings.network.shareCacheDuration.opt.days30' },
      ],
      allowCustom: true,
      customMin: 1000,
      customMax: 2592000000,
    },
  },
  {
    id: 'network.timeoutMode',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.timeoutMode.label',
    descriptionKey: 'settings.network.timeoutMode.description',
    keywords: ['timeout', 'network', 'slow', 'vpn', 'connection', 'latency'],
    type: 'enum',
    default: 'normal',
    component: 'radio',
    constraints: {
      options: [
        {
          value: 'normal',
          labelKey: 'settings.network.timeoutMode.opt.normal',
          descriptionKey: 'settings.network.timeoutMode.optDesc.normal',
        },
        {
          value: 'slow',
          labelKey: 'settings.network.timeoutMode.opt.slow',
          descriptionKey: 'settings.network.timeoutMode.optDesc.slow',
        },
        { value: 'custom', labelKey: 'settings.network.timeoutMode.opt.custom' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'network.customTimeout',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.customTimeout.label',
    descriptionKey: 'settings.network.customTimeout.description',
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
  {
    id: 'network.smbConcurrency',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.smbConcurrency.label',
    descriptionKey: 'settings.network.smbConcurrency.description',
    keywords: ['smb', 'concurrency', 'parallel', 'copy', 'batch', 'performance', 'transfer', 'speed'],
    type: 'number',
    default: 10,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 32,
      step: 1,
    },
  },

  // ========================================================================
  // File systems › MTP (Android/Kindle/cameras)
  // ========================================================================
  {
    id: 'fileOperations.mtpEnabled',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpEnabled.label',
    descriptionKey: 'settings.fileOperations.mtpEnabled.description',
    keywords: ['mtp', 'android', 'usb', 'device', 'phone', 'ptpcamerad', 'mobile'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileOperations.mtpConnectionWarning',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpConnectionWarning.label',
    descriptionKey: 'settings.fileOperations.mtpConnectionWarning.description',
    keywords: ['mtp', 'warning', 'notification', 'connect', 'toast', 'android'],
    type: 'boolean',
    default: true,
    component: 'checkbox',
  },

  // ========================================================================
  // File systems › Git
  // ========================================================================
  {
    id: 'fileExplorer.git.showRepoChip',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showRepoChip.label',
    descriptionKey: 'settings.fileExplorer.git.showRepoChip.description',
    keywords: ['git', 'chip', 'branch', 'breadcrumb', 'repo', 'status', 'ahead', 'behind', 'dirty'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showStatusColumn',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showStatusColumn.label',
    descriptionKey: 'settings.fileExplorer.git.showStatusColumn.description',
    keywords: ['git', 'status', 'column', 'modified', 'untracked', 'ignored', 'added', 'deleted'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showVirtualGitPortal',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showVirtualGitPortal.label',
    descriptionKey: 'settings.fileExplorer.git.showVirtualGitPortal.description',
    keywords: ['git', 'portal', 'virtual', 'branches', 'tags', 'commits', 'worktrees', 'history'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Viewer
  // ========================================================================
  {
    id: 'viewer.wordWrap',
    section: ['Viewer'],
    labelKey: 'settings.viewer.wordWrap.label',
    descriptionKey: 'settings.viewer.wordWrap.description',
    keywords: ['viewer', 'wrap', 'word', 'line', 'horizontal', 'scroll'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Developer › MCP server
  // ========================================================================
  {
    id: 'developer.mcpEnabled',
    section: ['Developer', 'MCP server'],
    labelKey: 'settings.developer.mcpEnabled.label',
    descriptionKey: 'settings.developer.mcpEnabled.description',
    keywords: ['mcp', 'server', 'ai', 'assistant', 'protocol', 'model'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'developer.mcpPort',
    section: ['Developer', 'MCP server'],
    labelKey: 'settings.developer.mcpPort.label',
    descriptionKey: 'settings.developer.mcpPort.description',
    keywords: ['port', 'mcp', 'network', 'ephemeral'],
    type: 'number',
    // 0 = ephemeral. The backend binds 127.0.0.1:0 and writes the actual port to
    // `<data_dir>/mcp.port` so external clients can discover it. Pinning a non-zero port
    // is still supported for tooling that needs a fixed target. See
    // `docs/tooling/instance-isolation.md` § "Per-resource breakdown" (Cmdr MCP HTTP port row).
    default: 0,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 65535,
      step: 1,
    },
  },

  // ========================================================================
  // Developer › Logging
  // ========================================================================
  {
    id: 'developer.verboseLogging',
    section: ['Developer', 'Logging'],
    labelKey: 'settings.developer.verboseLogging.label',
    descriptionKey: 'settings.developer.verboseLogging.description',
    keywords: ['log', 'debug', 'verbose', 'troubleshoot', 'performance', 'console'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Updates & privacy
  // ========================================================================
  {
    id: 'updates.autoCheck',
    section: ['Updates & privacy'],
    labelKey: 'settings.updates.autoCheck.label',
    descriptionKey: 'settings.updates.autoCheck.description',
    keywords: ['update', 'auto', 'check', 'version', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'whatsNew.showOnUpdate',
    section: ['Updates & privacy'],
    labelKey: 'settings.whatsNew.showOnUpdate.label',
    descriptionKey: 'settings.whatsNew.showOnUpdate.description',
    keywords: ['changelog', 'release notes', "what's new", 'update notes'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'whatsNew.lastSeenVersion',
    section: ['Advanced'],
    labelKey: 'settings.whatsNew.lastSeenVersion.label',
    descriptionKey: 'settings.whatsNew.lastSeenVersion.description',
    keywords: [],
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
  },
  {
    id: 'analytics.enabled',
    section: ['Updates & privacy'],
    labelKey: 'settings.analytics.enabled.label',
    descriptionKey: 'settings.analytics.enabled.description',
    keywords: ['analytics', 'usage', 'stats', 'privacy', 'telemetry', 'beta', 'tracking', 'opt-out'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'analytics.email',
    section: ['Updates & privacy'],
    labelKey: 'settings.analytics.email.label',
    descriptionKey: 'settings.analytics.email.description',
    keywords: ['email', 'beta', 'contact', 'newsletter', 'updates', 'survey'],
    type: 'string',
    default: '',
    component: 'text-input',
  },
  {
    id: 'updates.crashReports',
    section: ['Updates & privacy'],
    labelKey: 'settings.updates.crashReports.label',
    descriptionKey: 'settings.updates.crashReports.description',
    keywords: ['crash', 'report', 'privacy', 'telemetry', 'bug', 'error'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'updates.errorReports',
    section: ['Updates & privacy'],
    labelKey: 'settings.updates.errorReports.label',
    descriptionKey: 'settings.updates.errorReports.description',
    keywords: ['error', 'report', 'auto', 'send', 'privacy', 'telemetry', 'bug', 'log', 'snippet', 'diagnostics'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  {
    id: 'updates.attachEmailToReports',
    section: ['Advanced'],
    labelKey: 'settings.updates.attachEmailToReports.label',
    descriptionKey: 'settings.updates.attachEmailToReports.description',
    keywords: ['email', 'report', 'crash', 'error', 'contact', 'attach', 'reply', 'beta'],
    type: 'boolean',
    default: false,
    component: 'switch',
    showInAdvanced: true,
  },

  // ========================================================================
  // Advanced (auto-generated UI, `showInAdvanced: true`).
  //
  // Entries below carry a `section` path purely for reference / search; they
  // are excluded from the section tree and rendered only inside the Advanced
  // section. `network.smbConcurrency` above and the two `fileOperations.*`
  // entries below (maxConflictsToShow, progressUpdateInterval) keep their
  // natural section path AND surface here.
  // ========================================================================
  {
    id: 'advanced.prefetchBufferSize',
    section: ['Advanced'],
    labelKey: 'settings.advanced.prefetchBufferSize.label',
    descriptionKey: 'settings.advanced.prefetchBufferSize.description',
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
    labelKey: 'settings.advanced.virtualizationBufferRows.label',
    descriptionKey: 'settings.advanced.virtualizationBufferRows.description',
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
    labelKey: 'settings.advanced.virtualizationBufferColumns.label',
    descriptionKey: 'settings.advanced.virtualizationBufferColumns.description',
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
    labelKey: 'settings.advanced.fileWatcherDebounce.label',
    descriptionKey: 'settings.advanced.fileWatcherDebounce.description',
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
    id: 'advanced.diskSpaceChangeThreshold',
    section: ['Advanced'],
    labelKey: 'settings.advanced.diskSpaceChangeThreshold.label',
    descriptionKey: 'settings.advanced.diskSpaceChangeThreshold.description',
    keywords: ['disk', 'space', 'threshold', 'poll', 'refresh', 'status', 'bar'],
    type: 'number',
    default: 1,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 1000,
      step: 1,
    },
  },
  {
    id: 'fileViewer.suppressBinaryWarning',
    section: ['Advanced'],
    labelKey: 'settings.fileViewer.suppressBinaryWarning.label',
    descriptionKey: 'settings.fileViewer.suppressBinaryWarning.description',
    keywords: ['viewer', 'binary', 'image', 'pdf', 'raw', 'warning', 'banner', 'f3', 'quick', 'look'],
    type: 'boolean',
    default: false,
    component: 'switch',
    showInAdvanced: true,
  },
  {
    id: 'fileExplorer.suppressQuickLookHint',
    section: ['Advanced'],
    labelKey: 'settings.fileExplorer.suppressQuickLookHint.label',
    descriptionKey: 'settings.fileExplorer.suppressQuickLookHint.description',
    keywords: ['quick', 'look', 'preview', 'space', 'finder', 'hint', 'toast', 'reminder'],
    type: 'boolean',
    default: false,
    component: 'switch',
    showInAdvanced: true,
  },
  {
    id: 'fileExplorer.tabs.closedTabHistorySize',
    section: ['Advanced'],
    labelKey: 'settings.fileExplorer.tabs.closedTabHistorySize.label',
    descriptionKey: 'settings.fileExplorer.tabs.closedTabHistorySize.description',
    keywords: ['tab', 'closed', 'reopen', 'history', 'undo', 'pane'],
    type: 'number',
    default: 10,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },
  {
    id: 'advanced.dragThreshold',
    section: ['Advanced'],
    labelKey: 'settings.advanced.dragThreshold.label',
    descriptionKey: 'settings.advanced.dragThreshold.description',
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
    id: 'fileExplorer.typeToJump.resetDelay',
    section: ['Advanced'],
    labelKey: 'settings.fileExplorer.typeToJump.resetDelay.label',
    descriptionKey: 'settings.fileExplorer.typeToJump.resetDelay.description',
    keywords: ['type', 'jump', 'reset', 'delay', 'fuzzy', 'search', 'navigation', 'keystroke', 'buffer'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 300,
      max: 3000,
      step: 100,
    },
  },
  {
    id: 'fileOperations.maxConflictsToShow',
    section: ['Behavior', 'File operations'],
    labelKey: 'settings.fileOperations.maxConflictsToShow.label',
    descriptionKey: 'settings.fileOperations.maxConflictsToShow.description',
    keywords: ['conflict', 'max', 'limit', 'preview', 'operation'],
    type: 'number',
    default: 100,
    component: 'select',
    showInAdvanced: true,
    constraints: {
      // Numeric option labels are plain numerals, not translatable copy.
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
  {
    id: 'fileOperations.progressUpdateInterval',
    section: ['Behavior', 'File operations'],
    labelKey: 'settings.fileOperations.progressUpdateInterval.label',
    descriptionKey: 'settings.fileOperations.progressUpdateInterval.description',
    keywords: ['progress', 'update', 'interval', 'refresh', 'cpu', 'performance'],
    type: 'number',
    default: 500,
    component: 'slider',
    showInAdvanced: true,
    constraints: {
      min: 50,
      max: 5000,
      step: 50,
      sliderStops: [100, 250, 500, 1000, 2000],
    },
  },
  {
    id: 'advanced.maxLogStorageMb',
    section: ['Advanced'],
    labelKey: 'settings.advanced.maxLogStorageMb.label',
    descriptionKey: 'settings.advanced.maxLogStorageMb.description',
    keywords: ['log', 'storage', 'disk', 'mb', 'cap', 'rotation', 'error', 'report', 'privacy'],
    type: 'number',
    default: 200,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 5000,
      step: 50,
    },
  },
  {
    id: 'advanced.mountTimeout',
    section: ['Advanced'],
    labelKey: 'settings.advanced.mountTimeout.label',
    descriptionKey: 'settings.advanced.mountTimeout.description',
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
    id: 'advanced.filterSafeSaveArtifacts',
    section: ['Advanced'],
    labelKey: 'settings.advanced.filterSafeSaveArtifacts.label',
    descriptionKey: 'settings.advanced.filterSafeSaveArtifacts.description',
    keywords: ['smb', 'safe-save', 'artifact', 'temp', 'sb', 'filter', 'watcher'],
    type: 'boolean',
    default: true,
    component: 'switch',
    showInAdvanced: true,
  },
  {
    id: 'advanced.serviceResolveTimeout',
    section: ['Advanced'],
    labelKey: 'settings.advanced.serviceResolveTimeout.label',
    descriptionKey: 'settings.advanced.serviceResolveTimeout.description',
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
    id: 'search.recentSearches.maxCount',
    section: ['Advanced'],
    labelKey: 'settings.search.recentSearches.maxCount.label',
    descriptionKey: 'settings.search.recentSearches.maxCount.description',
    keywords: ['search', 'recent', 'history', 'cap', 'limit', 'max', 'count'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    id: 'selection.recentSelections.maxCount',
    section: ['Advanced'],
    labelKey: 'settings.selection.recentSelections.maxCount.label',
    descriptionKey: 'settings.selection.recentSelections.maxCount.description',
    keywords: ['selection', 'select', 'recent', 'history', 'cap', 'limit', 'max', 'count'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    id: 'onboarding.upgradeNudgeShown',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.upgradeNudgeShown.label',
    descriptionKey: 'settings.onboarding.upgradeNudgeShown.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'advanced.updateCheckInterval',
    section: ['Advanced'],
    labelKey: 'settings.advanced.updateCheckInterval.label',
    descriptionKey: 'settings.advanced.updateCheckInterval.description',
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
// Resolution: authored keys → rendered (getter-backed) definitions
//
// `label`/`description`/option labels are getters that resolve the catalog
// string through `t()` at READ time. So every `getSettingDefinition(...).label`
// consumer gets a rendered string (the pre-i18n behavior), reactivity works in
// markup, and snapshot semantics hold in plain `.ts` (matching the transfer
// pilot). An option with a literal `label` (brand names, numerals) passes
// through unchanged; option labels authored with a `labelKey` resolve lazily.
// ============================================================================

/** Resolves one authored option to a rendered `EnumOption` (getter-backed). */
function resolveOption(opt: EnumOptionSource | EnumOption): EnumOption {
  if ('label' in opt) return opt // literal label (brand names, numerals)
  const out: EnumOption = {
    value: opt.value,
    get label() {
      return tString(opt.labelKey)
    },
  }
  if (opt.descriptionKey !== undefined) {
    const descKey = opt.descriptionKey
    Object.defineProperty(out, 'description', { enumerable: true, get: () => tString(descKey) })
  }
  return out
}

/** Resolves authored constraints, mapping option keys to rendered options. */
function resolveConstraints(c: SettingConstraintsSource | undefined): SettingConstraints | undefined {
  if (!c) return undefined
  const { options, ...rest } = c
  if (!options) return rest
  return { ...rest, options: options.map(resolveOption) }
}

/** Turns an authored source into a `SettingDefinition` with resolved copy. */
function resolveDefinition(src: SettingDefinitionSource): SettingDefinition {
  const { labelKey, descriptionKey, constraints, ...rest } = src
  const def = {
    ...rest,
    constraints: resolveConstraints(constraints),
    get label() {
      return tString(labelKey)
    },
    get description() {
      return descriptionKey === undefined ? '' : tString(descriptionKey)
    },
  } as SettingDefinition
  return def
}

export const settingsRegistry: SettingDefinition[] = settingsRegistrySource.map(resolveDefinition)

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
  return settingsRegistry.filter((s) => s.showInAdvanced && !s.hidden)
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
      throw new SettingValidationError(id, `Custom value ${String(value)} is below minimum ${String(c.customMin)}`)
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
    if (setting.hidden) continue // Internal-only settings (e.g., network.firstTriggerDone)

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
