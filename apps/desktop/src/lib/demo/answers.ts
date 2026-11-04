/**
 * The demo's answers for every command that isn't a listing: what a fresh, licensed-for-personal-use
 * Cmdr on a Mac with no extras would say. Listing commands live in `listing.ts`, the volume list in
 * `volumes.ts`.
 */
import { asset } from '$app/paths'
import { refuse, type AnswerTable, type Payload, type VoidCommand } from './answer-types.js'
import type { ConfigureAiOutcome } from '#lib/tauri-commands/settings.js'

/** The bundled generic icon for an icon id: a folder for folder-ish ids (as `FileIcon.svelte` reads them), else a document. */
function genericIcon(iconId: string): string {
  const folder =
    iconId === 'dir' || iconId === 'symlink-dir' || iconId.startsWith('special:') || iconId.startsWith('path:')
  return asset(folder ? 'icons/default-folder.png' : 'icons/default-file.png')
}

const off = () => false
const on = () => true
const none = () => null
const nothing = () => undefined
const empty = () => []

/**
 * Commands outside the generated bindings (generic `<R: Runtime>` ones the wrappers in
 * `#lib/tauri-commands/` call through raw `invoke`), keyed by their snake_case wire name. Typed
 * against the wrapper's return type by hand, since no binding pins them.
 */
export const rawAnswers: Partial<Record<string, (payload: Payload) => unknown>> = {
  configure_ai: (): ConfigureAiOutcome => ({ secretStoreError: null }),
  // Menu and window syncs that return nothing (the wrappers in `app-state.ts` and `app.html`'s boot guard).
  show_main_window: nothing,
  set_ui_language: nothing,
  update_pin_tab_menu: nothing,
  set_reopen_closed_tab_enabled: nothing,
  update_view_mode_menu: nothing,
  set_file_operations_blocked: nothing,
  set_open_terminal_here_enabled: nothing,
  set_commands_refused_over_dialog: nothing,
  sync_menu_show_hidden: nothing,
  update_menu_context: nothing,
  update_select_same_kind_menu: nothing,
  update_undo_trash_menu: nothing,
  activate_window_menu: nothing,
  order_window_to_back: nothing,
}

/**
 * Fire-and-forget notices a demo session sends: settings pushed to the backend, menu and pane-state
 * syncs, telemetry, and logs. Nothing to do with them, so they're acknowledged without a log line.
 * `VoidCommand` proves each returns nothing; a command that starts returning a value drops out of
 * the type and needs a real answer.
 */
export const silentCommands: readonly VoidCommand[] = [
  'registerKnownDialogs',
  'setMainWindowVisible',
  'updateSortMenu',
  'notifyDialogOpened',
  'notifyDialogClosed',
  'setServersViewShown',
  'batchFeLogs',
  'recordSettingsDefaults',
  'updateFileWatcherDebounce',
  'trackEvent',
  'updateServiceResolveTimeout',
  'setErrorReportsEnabled',
  'setSecurityLogEnabled',
  'setShowVirtualGitPortal',
  'setDiskSpaceSizeFormat',
  'askCmdrEnabledChanged',
  'updateFocusedPane',
  'watchVolumeSpace',
  'unwatchVolumeSpace',
  'drainPendingReveals',
  'updateServicesSelection',
  'updatePaneTabs',
  'updateLeftPaneState',
  'updateRightPaneState',
  'recordBreadcrumb',
  'setChildWindowRect',
]

export const appAnswers: AnswerTable = {
  // Run mode and platform
  getAutomatedRun: () => 'none',
  isE2eMode: off,
  isForceOnboarding: off,
  getOsLocales: () => ({ ui: navigator.language, format: navigator.language }),
  getMacosMajorVersion: () => 26,
  getSystemTextSizeMultiplier: () => 1,
  getAccentColor: () => '#007aff',
  getGlassTintAmount: none,
  getDisplayAccessibility: () => ({
    reduceTransparency: false,
    increaseContrast: false,
    differentiateWithoutColor: false,
    invertColors: false,
    showBorders: false,
  }),
  getLocalizedSystemStrings: () => ({
    systemSettings: 'System Settings',
    privacyAndSecurity: 'Privacy & Security',
    fullDiskAccess: 'Full Disk Access',
    filesAndFolders: 'Files and Folders',
    localNetwork: 'Local Network',
    appearance: 'Appearance',
    automation: 'Automation',
  }),
  getPathLimits: () => ({ maxNameBytes: 255, maxPathBytes: 1024 }),
  getIsolatedStorePath: none,
  whatsNewDevOverride: none,

  // Permissions, license, and startup notices
  checkFullDiskAccess: on,
  checkFullDiskAccessQuiet: on,
  getLicenseStatus: () => ({ type: 'personal', showCommercialReminder: false }),
  needsLicenseValidation: off,
  getManagedPolicy: () => ({
    managed: false,
    usageStatsDisabled: false,
    reportsDisabled: false,
    securityLogRequired: false,
    // No updater in a browser: nothing to check for.
    updates: { kind: 'disabled' },
    ai: { mode: 'allowed', allowedCloudHosts: null },
    lockedSettings: [],
  }),
  takeStateRecoveries: empty,
  checkPendingCrashReport: none,
  getRestrictedPaths: empty,

  // Nudges and integrations with the rest of macOS: none apply in a browser
  getLaunchDayCount: () => 1,
  getRevealHandlerState: () => ({ state: { kind: 'notRegistered' }, blockedBy: null }),
  getDockPinState: () => ({ kind: 'alreadyPinned' }),
  getFinderExtensionStatus: none,
  getCliCommandStatus: none,
  getWhatsNew: empty,
  updateCheckDueIn: () => Number.MAX_SAFE_INTEGER,
  askCmdrLegacyOptIn: () => 'notRecorded',

  // The pane's surroundings
  // macOS draws the live icons, and the demo can't ask it: every icon is the app's own bundled
  // generic folder or document (`static/icons/default-*.png`), under the demo's base path.
  getIcons: (p) => ({
    data: Object.fromEntries((p.iconIds as string[]).map((id) => [id, genericIcon(id)])),
    timedOut: false,
  }),
  getGitRepoInfo: () => ({ data: null, timedOut: false }),
  // A browser has no apps to open a file in: the app's own refusal for a platform without an opener.
  defaultAppName: none,
  openPath: () => refuse('openPath', { type: 'unsupported' }),
  // No visit history to feed: the demo has no frecency store.
  recordVisit: none,
  // A demo panel opens centered every time (`windows.ts`): no remembered rect.
  getChildWindowRect: none,
  // No native menu to show the accelerator in.
  updateMenuAccelerator: none,
  // No Finder tags, packages, custom folder icons, or index-backed folder stats on the demo volume.
  enrichTags: () => ({ data: null, timedOut: false }),
  enrichPackages: () => ({ data: null, timedOut: false }),
  getCustomFolderIconIds: () => ({ data: [], timedOut: false }),
  getDirStatsBatch: (p) => (p.paths as string[]).map(() => null),
  getVolumeSpace: () => ({
    data: { kind: 'bounded', totalBytes: 994_662_584_320, availableBytes: 412_031_113_216, usedBytes: 582_631_471_104 },
    timedOut: false,
  }),
  getVolumeIndexStatusById: (p) => ({
    volumeId: p.volumeId as string,
    enabled: false,
    freshness: null,
    failure: null,
    scanCompletedAt: null,
    scanDurationMs: null,
    coalescedSignalsSinceSweep: 0,
    unreadableLocations: 0,
    unreadableRetried: false,
    unreadableSettled: false,
    nextSweepDueAt: null,
    liveWatch: false,
  }),

  // Background subsystems the demo doesn't run
  startIndexingAfterFdaDecision: none,
  listNetworkHosts: empty,
  getNetworkDiscoveryState: () => 'idle',
  latestOperationProgress: empty,
  suggestedOpsList: empty,
  agentWakeStatus: () => ({ phase: { phase: 'idle' }, readiness: 'askCmdrOff' }),
  getAiRuntimeStatus: () => ({
    serverRunning: false,
    serverStarting: false,
    pid: null,
    port: null,
    modelInstalled: false,
    modelName: '',
    modelSizeBytes: 0,
    downloadInProgress: false,
    localAiSupported: false,
    kvBytesPerToken: 0,
    baseOverheadBytes: 0,
  }),
  getBusyVolumeIds: empty,
  getEjectingVolumeIds: empty,
  listOperations: empty,
  getIndexStatus: () => ({
    initialized: false,
    scanning: false,
    coveredInPhases: false,
    walkedRoots: [],
    coveragePhase: null,
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    indexStatus: null,
    dbFileSize: null,
    volumeUsedBytes: null,
    scanRunKind: null,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    leftAfterFindFilesMs: null,
    leftAfterSaveMs: null,
    leftAfterComputeMs: null,
    leftAfterCatchUpMs: null,
  }),
  mediaIndexVolumeState: () => ({
    enabled: false,
    indexing: false,
    enrichedCount: 0,
    qualifyingCount: null,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: null,
    keptCount: null,
  }),
}
