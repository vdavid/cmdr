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
  // Behavior › File system watching
  // (formerly "Drive indexing"; renamed so the indexer and the downloads
  // watcher both live under one umbrella that shares the FDA gate)
  // ========================================================================
  {
    id: 'indexing.enabled',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.enabled.label',
    descriptionKey: 'settings.indexing.enabled.description',
    // The Drive-indexing card has no dedicated `card*` key; it reuses this row-label key
    // as its title (FileSystemWatchingSection.svelte). Same key as `indexing.indexSize`.
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Hidden search anchor (not a control). The "Index size / Clear index" action row is
    // hand-rendered in FileSystemWatchingSection.svelte with no registry entry of its own,
    // so search couldn't reach it and its card couldn't know to show. This anchor gives it
    // a searchable identity: `buildSearchIndex` keeps hidden entries, `buildSectionTree`
    // skips them, so it never adds a nav row. Never read or written. Its `section` MUST
    // equal the hosting page's, or the blank-page fix breaks (the anchor must land in that
    // page's section-scoped match set). Reuses the existing `indexSize` label key (no new
    // string). Additive key, so no SCHEMA_VERSION bump (defaults rebuild from the registry).
    id: 'indexing.indexSize',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.fileSystemWatching.indexSize',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['clear index', 'index database'],
    type: 'boolean',
    default: false,
    hidden: true,
  },
  {
    // Gates the per-drive first-connect "turn on indexing?" notification (D6).
    id: 'indexing.askForEachDrive',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.askForEachDrive.label',
    descriptionKey: 'settings.indexing.askForEachDrive.description',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['drive', 'index', 'ask', 'prompt', 'notification', 'connect', 'network', 'smb', 'usb'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Gates the one-time "your drive went stale" dialog (D2). The yellow badge
    // shows regardless of this toggle.
    id: 'indexing.staleNotify',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.staleNotify.label',
    descriptionKey: 'settings.indexing.staleNotify.description',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['drive', 'index', 'stale', 'outdated', 'notify', 'notification', 'disconnect', 'network', 'smb'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Internal (FE-owned): JSON array of volume ids the user silenced via
    // "Don't ask again for this drive". Never a UI row; the "Re-enable
    // notifications for all drives" button resets it to "[]".
    id: 'indexing.silencedDrives',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.silencedDrives.label',
    descriptionKey: 'settings.indexing.silencedDrives.description',
    keywords: [],
    type: 'string',
    default: '[]',
    component: 'text-input',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the one-time stale dialog has fired once.
    id: 'indexing.firstStaleDialogShown',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.indexing.firstStaleDialogShown.label',
    descriptionKey: 'settings.indexing.firstStaleDialogShown.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Master toggle for image-content (OCR) indexing. Off by default; live-applied to
    // the `media_index` backend scheduler via `set_image_index_enabled`. Its own card
    // in FileSystemWatchingSection.svelte, titled by `cardKey`.
    id: 'mediaIndex.enabled',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.enabled.label',
    descriptionKey: 'settings.mediaIndex.enabled.description',
    cardKey: 'settings.mediaIndex.card',
    keywords: ['image', 'photo', 'ocr', 'text', 'search', 'index', 'picture', 'screenshot', 'content'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    // Internal (FE-owned): JSON array of volume ids opted into background network (SMB)
    // image enrichment (network enrichment). Off by default per volume; the per-network-volume rows in
    // FileSystemWatchingSection's "Image search" card toggle it, persisting here AND
    // calling `media_index_set_network_volume_enabled`. Read by the Rust loader as an array.
    id: 'mediaIndex.networkVolumes',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.networkVolumes.label',
    descriptionKey: 'settings.mediaIndex.networkVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of volume ids marked "always index" (enrich
    // regardless of importance). Toggled by the per-network-volume rows; persisted here
    // AND pushed via `media_index_set_always_index_volume`.
    id: 'mediaIndex.alwaysIndexVolumes',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.alwaysIndexVolumes.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexVolumes.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of absolute OS-mount folder paths marked "always
    // index". Set by the per-folder override; persisted here AND pushed via
    // `media_index_set_always_index_folder`.
    id: 'mediaIndex.alwaysIndexFolders',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.alwaysIndexFolders.label',
    descriptionKey: 'settings.mediaIndex.alwaysIndexFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // Internal (FE-owned): JSON array of absolute OS folder paths EXCLUDED from image
    // indexing (the privacy veto). Set by the folder context-menu "Don't index images
    // in this folder" item; persisted here AND pushed via `media_index_set_excluded_folder`
    // (which also retro-deletes the folder's existing rows). Read by the Rust loader as
    // an array.
    id: 'mediaIndex.excludedFolders',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.excludedFolders.label',
    descriptionKey: 'settings.mediaIndex.excludedFolders.description',
    keywords: [],
    type: 'string-array',
    default: [],
    hidden: true,
  },
  {
    // The image-index importance threshold (`0.0..=1.0`): the lowest folder-importance
    // level the scheduler enriches. Rendered as named buckets by the bespoke
    // `MediaIndexImportanceSlider.svelte` inside the "Image search" card (not an auto
    // row), so `hidden`. Default `0.0` matches the backend `DEFAULT_IMPORTANCE_THRESHOLD`
    // (enrich every scored folder — non-regressive vs the OCR slice, junk is floored out anyway), so
    // the UI and a sparse (unpersisted) store agree without eagerly writing a default.
    // Live-applied via the `settings-applier.ts` passthrough → `media_index_set_importance_threshold`.
    id: 'mediaIndex.importanceThreshold',
    section: ['Behavior', 'File system watching'],
    labelKey: 'settings.mediaIndex.importanceThreshold.label',
    descriptionKey: 'settings.mediaIndex.importanceThreshold.description',
    keywords: ['image', 'photo', 'index', 'importance', 'folders', 'coverage', 'depth'],
    type: 'number',
    default: 0,
    hidden: true,
  },
  {
    id: 'behavior.fileSystemWatching.downloadsNotifications',
    section: ['Behavior', 'File system watching'],
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
    section: ['Behavior', 'File system watching'],
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
    section: ['Behavior', 'File system watching'],
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
