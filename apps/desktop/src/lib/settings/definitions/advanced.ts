/**
 * Advanced section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const advancedSettings: SettingDefinitionSource[] = [
  {
    // Verbose (debug-level) logging. Auto-renders as a switch in the "Logging and
    // diagnostics" card; that card's two action buttons (open log folder, copy
    // diagnostics) are NOT settings, so `AdvancedSection` appends them as the card's
    // trailing "extra content" (the marker-id-keyed block in `AdvancedSection.svelte`,
    // matched by this setting's id). Live-applied via `settings-applier.ts` →
    // `setVerboseLogging`. The `developer.*` id prefix is a stable persistence key;
    // homing the setting under Advanced doesn't touch it.
    id: 'developer.verboseLogging',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    labelKey: 'settings.developer.verboseLogging.label',
    descriptionKey: 'settings.developer.verboseLogging.description',
    keywordsKey: 'settings.developer.verboseLogging.keywords',
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'updates.attachEmailToReports',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    labelKey: 'settings.updates.attachEmailToReports.label',
    descriptionKey: 'settings.updates.attachEmailToReports.description',
    keywordsKey: 'settings.updates.attachEmailToReports.keywords',
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  {
    id: 'advanced.logLlmCalls',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    labelKey: 'settings.advanced.logLlmCalls.label',
    descriptionKey: 'settings.advanced.logLlmCalls.description',
    keywordsKey: 'settings.advanced.logLlmCalls.keywords',
    terms: ['llm', 'ask cmdr'],
    type: 'boolean',
    // Dev-on, prod-off: `import.meta.env.DEV` inlines to a build-time boolean, so a
    // developer's runs are inspectable out of the box while shipped builds stay off until
    // opt-in. The Rust default (`AtomicBool::new(cfg!(debug_assertions))`) matches, so the
    // two agree with or without the frontend push.
    default: import.meta.env.DEV,
    component: 'switch',
  },

  // ========================================================================
  // Advanced (auto-generated UI).
  //
  // `section: ['Advanced']` is the single home for these settings: they render
  // ONLY on the Advanced page (which auto-renders `getAdvancedSettings()`), never
  // mirrored onto a feature page. `cardKey` groups them into `SectionCard`s on
  // that page (one card per `cardKey`, see `AdvancedSection.svelte` +
  // `advanced-grouping.ts`). `getAdvancedSettings()` selects on `section[0]`, so
  // the page membership and the nav identity are the same `section`.
  // ========================================================================
  {
    id: 'advanced.prefetchBufferSize',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.performance',
    labelKey: 'settings.advanced.prefetchBufferSize.label',
    descriptionKey: 'settings.advanced.prefetchBufferSize.description',
    keywordsKey: 'settings.advanced.prefetchBufferSize.keywords',
    type: 'number',
    default: 200,
    component: 'number-input',
    constraints: {
      min: 50,
      max: 1000,
      step: 50,
    },
  },
  {
    id: 'advanced.virtualizationBufferRows',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.performance',
    labelKey: 'settings.advanced.virtualizationBufferRows.label',
    descriptionKey: 'settings.advanced.virtualizationBufferRows.description',
    keywordsKey: 'settings.advanced.virtualizationBufferRows.keywords',
    type: 'number',
    default: 20,
    component: 'number-input',
    constraints: {
      min: 5,
      max: 100,
      step: 5,
    },
  },
  {
    id: 'advanced.virtualizationBufferColumns',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.performance',
    labelKey: 'settings.advanced.virtualizationBufferColumns.label',
    descriptionKey: 'settings.advanced.virtualizationBufferColumns.description',
    keywordsKey: 'settings.advanced.virtualizationBufferColumns.keywords',
    type: 'number',
    default: 2,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 10,
      step: 1,
    },
  },
  {
    id: 'advanced.fileWatcherDebounce',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.fileWatching',
    labelKey: 'settings.advanced.fileWatcherDebounce.label',
    descriptionKey: 'settings.advanced.fileWatcherDebounce.description',
    keywordsKey: 'settings.advanced.fileWatcherDebounce.keywords',
    type: 'duration',
    default: 200,
    component: 'duration',
    constraints: {
      unit: 'ms',
      minMs: 50,
      maxMs: 2000,
    },
  },
  {
    id: 'advanced.diskSpaceChangeThreshold',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.fileWatching',
    labelKey: 'settings.advanced.diskSpaceChangeThreshold.label',
    descriptionKey: 'settings.advanced.diskSpaceChangeThreshold.description',
    keywordsKey: 'settings.advanced.diskSpaceChangeThreshold.keywords',
    type: 'number',
    default: 1,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 1000,
      step: 1,
    },
  },
  {
    id: 'fileViewer.suppressBinaryWarning',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.hintsAndWarnings',
    labelKey: 'settings.fileViewer.suppressBinaryWarning.label',
    descriptionKey: 'settings.fileViewer.suppressBinaryWarning.description',
    keywordsKey: 'settings.fileViewer.suppressBinaryWarning.keywords',
    terms: ['pdf', 'f3'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.suppressQuickLookHint',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.hintsAndWarnings',
    labelKey: 'settings.fileExplorer.suppressQuickLookHint.label',
    descriptionKey: 'settings.fileExplorer.suppressQuickLookHint.description',
    keywordsKey: 'settings.fileExplorer.suppressQuickLookHint.keywords',
    terms: ['finder'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.tabs.closedTabHistorySize',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.historyAndLimits',
    labelKey: 'settings.fileExplorer.tabs.closedTabHistorySize.label',
    descriptionKey: 'settings.fileExplorer.tabs.closedTabHistorySize.description',
    keywordsKey: 'settings.fileExplorer.tabs.closedTabHistorySize.keywords',
    type: 'number',
    default: 10,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },
  {
    id: 'advanced.dragThreshold',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.input',
    labelKey: 'settings.advanced.dragThreshold.label',
    descriptionKey: 'settings.advanced.dragThreshold.description',
    keywordsKey: 'settings.advanced.dragThreshold.keywords',
    type: 'number',
    default: 5,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },
  {
    // macOS's own default (Finder does it), but plenty of people find it jarring,
    // so it's a switch. `routes/(main)/escape-key.ts` owns the behavior.
    id: 'advanced.exitFullScreenOnEscape',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.input',
    labelKey: 'settings.advanced.exitFullScreenOnEscape.label',
    descriptionKey: 'settings.advanced.exitFullScreenOnEscape.description',
    keywordsKey: 'settings.advanced.exitFullScreenOnEscape.keywords',
    terms: ['esc'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileExplorer.typeToJump.resetDelay',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.input',
    labelKey: 'settings.fileExplorer.typeToJump.resetDelay.label',
    descriptionKey: 'settings.fileExplorer.typeToJump.resetDelay.description',
    keywordsKey: 'settings.fileExplorer.typeToJump.resetDelay.keywords',
    type: 'number',
    default: 1000,
    component: 'number-input',
    constraints: {
      min: 300,
      max: 3000,
      step: 100,
    },
  },
  {
    id: 'fileOperations.maxConflictsToShow',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.fileOperations',
    labelKey: 'settings.fileOperations.maxConflictsToShow.label',
    descriptionKey: 'settings.fileOperations.maxConflictsToShow.description',
    keywordsKey: 'settings.fileOperations.maxConflictsToShow.keywords',
    type: 'number',
    default: 100,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 1000,
      step: 1,
    },
  },
  {
    id: 'fileOperations.progressUpdateInterval',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.fileOperations',
    labelKey: 'settings.fileOperations.progressUpdateInterval.label',
    descriptionKey: 'settings.fileOperations.progressUpdateInterval.description',
    keywordsKey: 'settings.fileOperations.progressUpdateInterval.keywords',
    terms: ['cpu'],
    type: 'number',
    default: 500,
    component: 'number-input',
    constraints: {
      min: 50,
      max: 5000,
      step: 50,
    },
  },
  {
    id: 'advanced.maxLogStorageMb',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    labelKey: 'settings.advanced.maxLogStorageMb.label',
    descriptionKey: 'settings.advanced.maxLogStorageMb.description',
    keywordsKey: 'settings.advanced.maxLogStorageMb.keywords',
    terms: ['mb'],
    type: 'number',
    default: 200,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 5000,
      step: 50,
    },
  },
  {
    id: 'advanced.mountTimeout',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.networkAndMounts',
    labelKey: 'settings.advanced.mountTimeout.label',
    descriptionKey: 'settings.advanced.mountTimeout.description',
    keywordsKey: 'settings.advanced.mountTimeout.keywords',
    type: 'duration',
    default: 20000,
    component: 'duration',
    constraints: {
      unit: 's',
      minMs: 5000,
      maxMs: 120000,
    },
  },
  {
    id: 'advanced.showStagingTempFiles',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.networkAndMounts',
    labelKey: 'settings.advanced.showStagingTempFiles.label',
    descriptionKey: 'settings.advanced.showStagingTempFiles.description',
    keywordsKey: 'settings.advanced.showStagingTempFiles.keywords',
    terms: ['cmdr-tmp'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'advanced.showSafeSaveFiles',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.networkAndMounts',
    labelKey: 'settings.advanced.showSafeSaveFiles.label',
    descriptionKey: 'settings.advanced.showSafeSaveFiles.description',
    keywordsKey: 'settings.advanced.showSafeSaveFiles.keywords',
    terms: ['sb', 'textedit'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'advanced.serviceResolveTimeout',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.networkAndMounts',
    labelKey: 'settings.advanced.serviceResolveTimeout.label',
    descriptionKey: 'settings.advanced.serviceResolveTimeout.description',
    keywordsKey: 'settings.advanced.serviceResolveTimeout.keywords',
    terms: ['bonjour', 'mdns'],
    type: 'duration',
    default: 5000,
    component: 'duration',
    constraints: {
      unit: 's',
      minMs: 1000,
      maxMs: 30000,
    },
  },
  {
    id: 'network.smbConcurrency',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.networkAndMounts',
    labelKey: 'settings.network.smbConcurrency.label',
    descriptionKey: 'settings.network.smbConcurrency.description',
    keywordsKey: 'settings.network.smbConcurrency.keywords',
    terms: ['smb'],
    type: 'number',
    default: 10,
    component: 'number-input',
    constraints: {
      min: 1,
      max: 32,
      step: 1,
    },
  },
  {
    id: 'search.recentSearches.maxCount',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.historyAndLimits',
    labelKey: 'settings.search.recentSearches.maxCount.label',
    descriptionKey: 'settings.search.recentSearches.maxCount.description',
    keywordsKey: 'settings.search.recentSearches.maxCount.keywords',
    type: 'number',
    default: 1000,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    id: 'selection.recentSelections.maxCount',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.historyAndLimits',
    labelKey: 'settings.selection.recentSelections.maxCount.label',
    descriptionKey: 'settings.selection.recentSelections.maxCount.description',
    keywordsKey: 'settings.selection.recentSelections.maxCount.keywords',
    type: 'number',
    default: 1000,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    // One-shot for the best-effort notice a Mac below `SUPPORTED_MACOS_MAJOR`
    // gets (`#lib/utils/webkit-compat.ts`). Hidden for the same reason the
    // onboarding flags are: it records what the user has already been told, so
    // re-telling them is the bug, and a Settings row would only invite that.
    id: 'advanced.oldMacosNoticeShown',
    section: ['Advanced'],
    labelKey: 'settings.advanced.oldMacosNoticeShown.label',
    descriptionKey: 'settings.advanced.oldMacosNoticeShown.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Hidden like `advanced.oldMacosNoticeShown`: it records that the user was told
    // once, and the toast itself links to the real switch.
    id: 'advanced.exitFullScreenOnEscapeHintShown',
    section: ['Advanced'],
    labelKey: 'settings.advanced.exitFullScreenOnEscapeHintShown.label',
    descriptionKey: 'settings.advanced.exitFullScreenOnEscapeHintShown.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'onboarding.upgradeNudgeShown',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.upgradeNudgeShown.label',
    descriptionKey: 'settings.onboarding.upgradeNudgeShown.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // A "no" to cloud AI that `main.db` refused to record, held here until the store takes it
    // (`lib/ai/cloud-consent.svelte.ts` holds, retries, and lets go). Hidden for the same
    // reason as onboarding's flags below: a row offering to flip it would be a way to lie
    // about consent. ⚠️ Every Rust cloud gate reads it fresh from `settings.json`
    // (`ai::cloud_consent::RevokePending`), so the "no" holds before the store catches up.
    id: 'ai.cloudConsentRevokePending',
    section: ['Advanced'],
    labelKey: 'settings.ai.cloudConsentRevokePending.label',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
    // An MCP client clearing it would undo the person's "no".
    mcpSettable: false,
  },
  {
    // The legacy held "no" to Ask Cmdr's old opt-in. Nothing writes it any more; its only
    // reader is the one-time `askCmdr.enabled` mapping (`ask_cmdr_legacy_opt_in`), where a
    // held "no" maps to off. Kept registered so an install that still carries it validates.
    id: 'askCmdr.consentRevokePending',
    section: ['Advanced'],
    labelKey: 'settings.askCmdr.consentRevokePending.label',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
    // An MCP client clearing it would undo the person's "no".
    mcpSettable: false,
  },
  {
    // Onboarding's own internal state. Hidden: these record what the user already
    // answered, so re-asking is the bug, and a Settings row offering to flip them
    // would be a way to lie about consent. Written from the wizard steps and the
    // startup gates; `onboarding.fullDiskAccessChoice` is also read by Rust at
    // startup (`settings/loader.rs`).
    id: 'onboarding.completed',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.completed.label',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'onboarding.fullDiskAccessChoice',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.fullDiskAccessChoice.label',
    // A `string`, not an `enum`: an enum's options each need a rendered label, and
    // nothing ever renders these. `FullDiskAccessChoice` keeps the three tokens
    // type-safe at every call site.
    type: 'string',
    default: 'unanswered',
    component: 'text-input',
    hidden: true,
  },
  {
    id: 'onboarding.termsAcceptedVersion',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.termsAcceptedVersion.label',
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
    // The person's acceptance of the terms: an MCP client must never write or clear it.
    mcpSettable: false,
  },
  {
    id: 'onboarding.termsAcceptedAt',
    section: ['Advanced'],
    labelKey: 'settings.onboarding.termsAcceptedAt.label',
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
    // The person's acceptance of the terms: an MCP client must never write or clear it.
    mcpSettable: false,
  },
  {
    id: 'advanced.updateCheckInterval',
    section: ['Advanced'],
    cardKey: 'settings.advanced.card.updates',
    labelKey: 'settings.advanced.updateCheckInterval.label',
    descriptionKey: 'settings.advanced.updateCheckInterval.description',
    keywordsKey: 'settings.advanced.updateCheckInterval.keywords',
    type: 'duration',
    default: 10_800_000, // 3 hours
    component: 'duration',
    constraints: {
      unit: 'min',
      minMs: 300000, // 5 min
      maxMs: 86400000, // 24 hours
    },
  },
]
