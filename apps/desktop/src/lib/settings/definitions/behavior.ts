/**
 * Behavior section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const behaviorSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Behavior › Navigation & file ops
  // ========================================================================
  {
    id: 'behavior.doubleClickPaneNavigatesToParent',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.navigation',
    labelKey: 'settings.behavior.doubleClickPaneNavigatesToParent.label',
    descriptionKey: 'settings.behavior.doubleClickPaneNavigatesToParent.description',
    keywords: [
      'double-click',
      'doubleclick',
      'double click',
      'pane',
      'background',
      'empty',
      'parent',
      'folder',
      'up',
      'navigate',
      'navigation',
    ],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Internal (FE-owned): whether the one-time "what just happened?" toast has
    // fired. Set to true the first time a background double-click navigates up.
    // No UI row; hidden from search and the section tree.
    id: 'behavior.doubleClickOnPaneNotificationSeen',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.doubleClickOnPaneNotificationSeen.label',
    descriptionKey: 'settings.behavior.doubleClickOnPaneNotificationSeen.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'fileOperations.allowFileExtensionChanges',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.fileOperations',
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
  {
    id: 'fileOperations.pasteClipboardAsFile',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.fileOperations',
    labelKey: 'settings.fileOperations.pasteClipboardAsFile.label',
    descriptionKey: 'settings.fileOperations.pasteClipboardAsFile.description',
    keywords: ['paste', 'clipboard', 'image', 'text', 'pdf', 'screenshot', 'file', 'create'],
    type: 'enum',
    default: 'createFileAndRename',
    component: 'radio',
    constraints: {
      options: [
        { value: 'doNothing', labelKey: 'settings.fileOperations.pasteClipboardAsFile.opt.doNothing' },
        { value: 'createFile', labelKey: 'settings.fileOperations.pasteClipboardAsFile.opt.createFile' },
        {
          value: 'createFileAndRename',
          labelKey: 'settings.fileOperations.pasteClipboardAsFile.opt.createFileAndRename',
        },
      ],
    },
  },

  // ------------------------------------------------------------------------
  // Operation log (retention), rendered as a card inside Navigation & file ops.
  //
  // Both settings are read by the Rust retention loop each prune tick
  // (`settings::load_operation_log_retention_limits`), so a change live-applies
  // with no restart and no `settings-applier` case. Modeled on
  // `network.shareCacheDuration` (preset select + custom). Age is a duration in
  // ms where `0` is the "Forever" sentinel (never prune by age); size is a byte
  // budget. Keys and units are the retention contract — see `operation_log/DETAILS.md`.
  // ------------------------------------------------------------------------
  {
    id: 'operationLog.maxAge',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.operationLog',
    labelKey: 'settings.operationLog.maxAge.label',
    descriptionKey: 'settings.operationLog.maxAge.description',
    keywords: ['operation', 'log', 'history', 'retention', 'age', 'keep', 'prune', 'forever', 'days', 'undo'],
    type: 'duration',
    default: 0, // 0 ms = the "Forever" sentinel (never prune by age)
    component: 'select',
    constraints: {
      unit: 'd',
      options: [
        { value: 0, labelKey: 'settings.operationLog.maxAge.opt.forever' },
        { value: 2592000000, labelKey: 'settings.operationLog.maxAge.opt.days30' },
        { value: 7776000000, labelKey: 'settings.operationLog.maxAge.opt.days90' },
        { value: 31536000000, labelKey: 'settings.operationLog.maxAge.opt.year1' },
      ],
      allowCustom: true,
      customMin: 3600000, // 1 hour
      customMax: 315360000000, // 10 years
    },
  },
  {
    id: 'operationLog.maxSize',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.operationLog',
    labelKey: 'settings.operationLog.maxSize.label',
    descriptionKey: 'settings.operationLog.maxSize.description',
    keywords: ['operation', 'log', 'history', 'retention', 'size', 'disk', 'space', 'limit', 'megabytes', 'gigabytes'],
    type: 'number',
    default: 3221225472, // 3 GB (binary), the default retention budget
    component: 'select',
    constraints: {
      options: [
        { value: 104857600, labelKey: 'settings.operationLog.maxSize.opt.mb100' },
        { value: 262144000, labelKey: 'settings.operationLog.maxSize.opt.mb250' },
        { value: 1073741824, labelKey: 'settings.operationLog.maxSize.opt.gb1' },
        { value: 2147483648, labelKey: 'settings.operationLog.maxSize.opt.gb2' },
        { value: 3221225472, labelKey: 'settings.operationLog.maxSize.opt.gb3' },
        { value: 5368709120, labelKey: 'settings.operationLog.maxSize.opt.gb5' },
      ],
      allowCustom: true,
      customMin: 10485760, // 10 MB
      customMax: 107374182400, // 100 GB
    },
  },

  // ========================================================================
  // Behavior › Archives
  // Per-format Enter behavior (Browse | Open | Ask) for archives and macOS
  // bundles. Stored as a pinned-shape JSON object keyed by format
  // (`{ zip: 'ask', bundle: 'ask' }`); parsed and rendered by the custom
  // `ArchivesSection`. FE-owned: read at Enter time, applies with no restart or
  // backend round-trip.
  // ========================================================================
  {
    id: 'behavior.archiveEnterBehavior',
    section: ['Behavior', 'Archives'],
    labelKey: 'settings.archives.enterBehavior.label',
    descriptionKey: 'settings.archives.enterBehavior.description',
    keywords: ['archive', 'zip', 'bundle', 'app', 'browse', 'open', 'extract', 'enter', 'launch'],
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    id: 'behavior.archiveCompressionLevel',
    section: ['Behavior', 'Archives'],
    labelKey: 'settings.archives.compressionLevel.label',
    descriptionKey: 'settings.archives.compressionLevel.description',
    keywords: ['compression', 'level', 'zip', 'deflate', 'archive', 'size', 'faster', 'smaller'],
    type: 'number',
    default: 6,
    component: 'slider',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  },

  // ========================================================================
  // Behavior › Notifications
  // The downloads watcher and the low-disk-space warning: both surface
  // notifications, and the downloads watcher shares the FDA gate. Rendered by
  // `NotificationsSection.svelte`. (The `*.fileSystemWatching.*` id prefix is a
  // stable persistence key; renaming the section doesn't touch it.)
  // ========================================================================
  {
    id: 'behavior.fileSystemWatching.downloadsNotifications',
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.description',
    cardKey: 'settings.fileSystemWatching.cardDownloads',
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
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description',
    cardKey: 'settings.fileSystemWatching.cardDownloads',
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
    section: ['Behavior', 'Notifications'],
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
    section: ['Behavior', 'Notifications'],
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
    section: ['Behavior', 'Notifications'],
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
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.description',
    cardKey: 'settings.fileSystemWatching.cardLowDiskSpace',
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
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceThresholdPercent.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceThresholdPercent.description',
    cardKey: 'settings.fileSystemWatching.cardLowDiskSpace',
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
]
