/**
 * Behavior section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { EnumOption, SettingDefinitionSource } from '../types'
import { formatRoundSize } from '#lib/units/byte-size.js'

/**
 * A preset size option built in base 1024, labelled with its IEC symbol in the UI
 * language ("100 MiB", "3 GiB"). A getter, so a language change relabels it live.
 */
function binarySizeOption(byteCount: number): EnumOption {
  return {
    value: byteCount,
    get label() {
      return formatRoundSize(byteCount, 'binary')
    },
  }
}

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
    keywordsKey: 'settings.behavior.doubleClickPaneNavigatesToParent.keywords',
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
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the one-time "your Network group is getting
    // long" toast has fired. Set the first time five server places are pinned.
    // No UI row; hidden from search and the section tree, the way
    // `behavior.doubleClickOnPaneNotificationSeen` is.
    id: 'behavior.serversPinHintSeen',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.serversPinHintSeen.label',
    descriptionKey: 'settings.behavior.serversPinHintSeen.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the volume switcher's favorites section was
    // last left open. Driven entirely by the section's own row; ONE value for
    // both panes, so the two switchers never disagree about it.
    id: 'behavior.switcherFavoritesExpanded',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.switcherFavoritesExpanded.label',
    descriptionKey: 'settings.behavior.switcherFavoritesExpanded.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal (FE-owned): when the one-time "keep Cmdr in your Dock?" offer was
    // made, as an ISO 8601 instant. Stamped when the toast is RAISED, so a crash
    // mid-toast can't make it reappear forever. A date rather than a flag because
    // it also feeds the shared nudge cooldown (`#lib/nudges/CLAUDE.md`). No UI
    // row; hidden the way the seen-flags above it are.
    id: 'behavior.dockPinNudgeOfferedAt',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.dockPinNudgeOfferedAt.label',
    descriptionKey: 'settings.behavior.dockPinNudgeOfferedAt.description',
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
  },
  {
    // Internal (FE-owned): when the one-time "open Show in Finder in Cmdr?" offer
    // was made, as an ISO 8601 instant. Same shape and the same cooldown as the
    // Dock stamp above.
    id: 'behavior.revealNudgeOfferedAt',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.revealNudgeOfferedAt.label',
    descriptionKey: 'settings.behavior.revealNudgeOfferedAt.description',
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the once-ever notice that fires the FIRST time
    // a reveal actually lands in Cmdr has been shown. ❌ Not a nudge: it answers
    // something the user set up themselves, so it takes no part in the cooldown.
    id: 'behavior.revealActivationNoticeSeen',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.revealActivationNoticeSeen.label',
    descriptionKey: 'settings.behavior.revealActivationNoticeSeen.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal (FE-owned): whether the "turn on USB debugging" line in an MTP
    // pane's header has been dismissed. Set once, by the line's own × button.
    // No UI row; hidden the way the two seen-flags above it are.
    id: 'behavior.adbHintDismissed',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.adbHintDismissed.label',
    descriptionKey: 'settings.behavior.adbHintDismissed.description',
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
    keywordsKey: 'settings.fileOperations.allowFileExtensionChanges.keywords',
    type: 'enum',
    default: 'ask',
    component: 'toggle-group',
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
    keywordsKey: 'settings.fileOperations.pasteClipboardAsFile.keywords',
    terms: ['pdf'],
    type: 'enum',
    default: 'createFileAndRename',
    component: 'toggle-group',
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
  {
    // FE-owned: read by `DualPaneExplorer` into the dialog state, where
    // `pane/confirmation-skip.ts` decides which confirmations it may leave out.
    // A permanent delete always asks, whatever this says.
    id: 'fileOperations.skipConfirmation',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.fileOperations',
    labelKey: 'settings.fileOperations.skipConfirmation.label',
    descriptionKey: 'settings.fileOperations.skipConfirmation.description',
    keywordsKey: 'settings.fileOperations.skipConfirmation.keywords',
    terms: ['F5', 'F6', 'F8'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ------------------------------------------------------------------------
  // Text editor, rendered as its own card inside Navigation & file ops, above
  // Terminal.
  //
  // FE-owned like the terminal below: `#lib/text-editor` reads the choice and
  // passes it to `openInEditor` / `listTextEditors`, so there's no applier case
  // and no backend push. The stored value is one string, told apart structurally
  // by Rust's `parse_choice`: `system` for the macOS plain-text default (`open
  // -t`), a bundle id for a listed editor, or an absolute `.app` path for a
  // "Choose an app…" pick. ❌ Never on `CATEGORICAL_STRING_KEYS`: the value can be
  // a path inside someone's home folder.
  // ------------------------------------------------------------------------
  {
    id: 'behavior.textEditorApp',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.textEditor',
    labelKey: 'settings.behavior.textEditorApp.label',
    descriptionKey: 'settings.behavior.textEditorApp.description',
    keywordsKey: 'settings.behavior.textEditorApp.keywords',
    terms: ['F4', 'Sublime Text', 'VS Code', 'BBEdit', 'TextEdit'],
    type: 'string',
    // Mirrors `SYSTEM_DEFAULT_EDITOR_CHOICE` in `#lib/text-editor/text-editor-choice.ts`
    // and `SYSTEM_DEFAULT_CHOICE` in `src-tauri/src/file_system/text_editor.rs`.
    default: 'system',
    // The options are whatever macOS lists right now, so they can't be registry
    // constants.
    component: 'select',
    // Linux keeps `xdg-open` and renders no Text editor card.
    macOSOnly: true,
  },
  {
    // Internal (FE-owned): whether the one-time "want a different editor next
    // time?" toast has fired after F4. No UI row; hidden from the section tree,
    // the way `behavior.openTerminalHereToastSeen` is. Hidden settings still
    // enter search, hence `macOSOnly` too.
    id: 'behavior.textEditorHintSeen',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.textEditorHintSeen.label',
    descriptionKey: 'settings.behavior.textEditorHintSeen.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
    macOSOnly: true,
  },

  // ------------------------------------------------------------------------
  // Terminal, rendered as its own card inside Navigation & file ops.
  //
  // FE-owned: the frontend reads the choice and passes it to `listTerminalApps`
  // / `openTerminalHere`, so there's no applier case and no backend push. The
  // stored value is one string holding either kind of choice, told apart
  // structurally by Rust's `parse_choice`: a bundle id for a known terminal, an
  // absolute `.app` path for a "Choose an app…" pick. Which apps exist and what
  // each is called comes from `list_terminal_apps`, never from a table here.
  // ------------------------------------------------------------------------
  {
    id: 'behavior.openTerminalHereApp',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.terminal',
    labelKey: 'settings.behavior.openTerminalHereApp.label',
    descriptionKey: 'settings.behavior.openTerminalHereApp.description',
    keywordsKey: 'settings.behavior.openTerminalHereApp.keywords',
    terms: ['iTerm', 'Warp', 'Ghostty', 'kitty', 'Alacritty', 'WezTerm', 'Hyper'],
    type: 'string',
    // Terminal.app, always present on macOS, so a user with no other terminal
    // never has to touch this row. Mirrors `TERMINAL_APP_BUNDLE_ID` in
    // `src-tauri/src/file_system/terminal.rs`.
    default: 'com.apple.Terminal',
    // Rendered by `TerminalAppSelect.svelte`, not `SettingSelect`: the options
    // are whatever is installed right now, so they can't be registry constants.
    component: 'select',
    // The whole feature is macOS-only, and so is the Terminal card.
    macOSOnly: true,
  },
  {
    // Internal (FE-owned): whether the one-time "which app opened, and how to
    // change it" toast has fired. No UI row; hidden from search and the section
    // tree, the way `behavior.doubleClickOnPaneNotificationSeen` is.
    id: 'behavior.openTerminalHereToastSeen',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.behavior.openTerminalHereToastSeen.label',
    descriptionKey: 'settings.behavior.openTerminalHereToastSeen.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },

  // ------------------------------------------------------------------------
  // Operation log, rendered as a card inside Navigation & file ops.
  //
  // `operationLog.enabled` is the recording switch: off stops journaling NEW
  // operations, so they can't be undone. The backend decides per operation at
  // its start, so a running one is never half-recorded, and history stays
  // readable and keeps being pruned (`crates/cmdr-operation-log/src/recording.rs`). Seeded from
  // `settings.json` at startup, live via `set_operation_log_enabled`.
  //
  // The two retention settings are read by the Rust retention loop each prune tick
  // (`settings::load_operation_log_retention_limits`), so a change live-applies
  // with no restart and no `settings-applier` case. Modeled on
  // `network.shareCacheDuration` (preset select + custom). Age is a duration in
  // ms where `0` is the "Forever" sentinel (never prune by age); size is a byte
  // budget. Keys and units are the retention contract — see `crates/cmdr-operation-log/DETAILS.md`.
  // ------------------------------------------------------------------------
  {
    id: 'operationLog.enabled',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.operationLog',
    labelKey: 'settings.operationLog.enabled.label',
    descriptionKey: 'settings.operationLog.enabled.description',
    keywordsKey: 'settings.operationLog.enabled.keywords',
    type: 'boolean',
    default: true,
    component: 'switch',
    // An MCP client that could turn this off would hide what it does next, and take undo with it.
    mcpSettable: false,
  },
  {
    id: 'operationLog.maxAge',
    section: ['Behavior', 'Navigation & file ops'],
    cardKey: 'settings.navigationAndFileOps.card.operationLog',
    labelKey: 'settings.operationLog.maxAge.label',
    descriptionKey: 'settings.operationLog.maxAge.description',
    keywordsKey: 'settings.operationLog.maxAge.keywords',
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
    keywordsKey: 'settings.operationLog.maxSize.keywords',
    type: 'number',
    default: 3221225472, // 3 GiB, the default retention budget
    component: 'select',
    constraints: {
      // Binary values, so the labels name binary units whatever the display setting is.
      options: [
        binarySizeOption(104857600), // 100 MiB
        binarySizeOption(262144000), // 250 MiB
        binarySizeOption(1073741824), // 1 GiB
        binarySizeOption(2147483648), // 2 GiB
        binarySizeOption(3221225472), // 3 GiB
        binarySizeOption(5368709120), // 5 GiB
      ],
      allowCustom: true,
      customMin: 10485760, // 10 MiB
      customMax: 107374182400, // 100 GiB
    },
  },

  // ========================================================================
  // Behavior › Archives
  // What pressing Enter does (Browse | Open | Ask), one setting per format.
  // ❗ The format list, the matcher for each, and these defaults live in
  // `file-explorer/pane/archive-enter-policy.ts`; a two-way parity test there
  // fails if a format loses its setting or a setting loses its format, so adding
  // `tar` means adding BOTH. FE-owned: read at Enter time, applies with no
  // restart or backend round-trip.
  // ========================================================================
  {
    id: 'behavior.archiveEnter.zip',
    section: ['Behavior', 'Archives'],
    cardKey: 'settings.archives.card.archives',
    labelKey: 'settings.archives.zip.label',
    descriptionKey: 'settings.archives.zip.description',
    keywordsKey: 'settings.behavior.archiveEnter.zip.keywords',
    terms: ['zip'],
    type: 'enum',
    default: 'ask',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'browse', labelKey: 'settings.archives.opt.browse' },
        { value: 'open', labelKey: 'settings.archives.opt.open' },
        { value: 'ask', labelKey: 'settings.archives.opt.ask' },
      ],
    },
  },
  {
    id: 'behavior.archiveEnter.ooxml',
    section: ['Behavior', 'Archives'],
    cardKey: 'settings.archives.card.archives',
    labelKey: 'settings.archives.ooxml.label',
    descriptionKey: 'settings.archives.ooxml.description',
    keywordsKey: 'settings.behavior.archiveEnter.ooxml.keywords',
    terms: ['docx', 'xlsx', 'pptx', 'jar', 'apk', 'office', 'word', 'excel', 'powerpoint'],
    type: 'enum',
    default: 'open',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'browse', labelKey: 'settings.archives.opt.browse' },
        { value: 'open', labelKey: 'settings.archives.opt.open' },
        { value: 'ask', labelKey: 'settings.archives.opt.ask' },
      ],
    },
  },
  {
    id: 'behavior.archiveEnter.bundle',
    section: ['Behavior', 'Archives'],
    cardKey: 'settings.archives.card.bundles',
    labelKey: 'settings.archives.bundle.label',
    descriptionKey: 'settings.archives.bundle.description',
    keywordsKey: 'settings.behavior.archiveEnter.bundle.keywords',
    type: 'enum',
    default: 'ask',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'browse', labelKey: 'settings.archives.opt.browse' },
        { value: 'open', labelKey: 'settings.archives.opt.open' },
        { value: 'ask', labelKey: 'settings.archives.opt.ask' },
      ],
    },
  },
  {
    id: 'behavior.archiveCompressionLevel',
    section: ['Behavior', 'Archives'],
    cardKey: 'settings.archives.card.archives',
    labelKey: 'settings.archives.compressionLevel.label',
    descriptionKey: 'settings.archives.compressionLevel.description',
    keywordsKey: 'settings.behavior.archiveCompressionLevel.keywords',
    terms: ['zip', 'deflate'],
    type: 'number',
    default: 6,
    component: 'slider',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  },

  // ========================================================================
  // Behavior › Notifications
  // How long on-screen notifications (toasts) stay, then the downloads watcher
  // and the low-disk-space warning: both surface notifications, and the
  // downloads watcher shares the FDA gate. Rendered by
  // `NotificationsSection.svelte`. (The `*.fileSystemWatching.*` id prefix is a
  // stable persistence key; renaming the section doesn't touch it.)
  // ========================================================================
  {
    // Read by `ToastContainer` (`#lib/ui/toast/`): on, no toast goes on a timer.
    id: 'behavior.notifications.keepUntilClosed',
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.notifications.keepUntilClosed.label',
    descriptionKey: 'settings.behavior.notifications.keepUntilClosed.description',
    cardKey: 'settings.notifications.cardOnScreen',
    keywordsKey: 'settings.behavior.notifications.keepUntilClosed.keywords',
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'behavior.fileSystemWatching.downloadsNotifications',
    section: ['Behavior', 'Notifications'],
    labelKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.label',
    descriptionKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.description',
    cardKey: 'settings.fileSystemWatching.cardDownloads',
    keywordsKey: 'settings.behavior.fileSystemWatching.downloadsNotifications.keywords',
    terms: ['macos'],
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
    keywordsKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.keywords',
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
    keywordsKey: 'settings.behavior.fileSystemWatching.globalGoToLatestShortcut.binding.keywords',
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
    keywordsKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceNotifications.keywords',
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
    keywordsKey: 'settings.behavior.fileSystemWatching.lowDiskSpaceThresholdPercent.keywords',
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
    keywordsKey: 'settings.search.autoApply.keywords',
    terms: ['regex'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
]
