// Re-export all modules for backward compatibility
// This allows existing imports from '$lib/tauri-commands' to continue working

// File listing (on-demand virtual scrolling API, sync status, font metrics)
export {
  listDirectoryStart,
  cancelListing,
  resortListing,
  getFileRange,
  getTotalCount,
  findFileIndex,
  findFileIndices,
  findFirstFuzzyMatch,
  getFileAt,
  getPathsAtIndices,
  getFilesAtIndices,
  listDirectoryEnd,
  refreshListing,
  getListingStats,
  refreshListingIndexSizes,
  startSelectionDrag,
  startDragPaths,
  prepareSelfDragOverlay,
  clearSelfDragOverlay,
  setSelfDragResolvedOperation,
  getPathLimits,
  pathExists,
  pathExistsChecked,
  statPathsKinds,
  createDirectory,
  createFile,
  getSyncStatus,
  storeFontMetrics,
  hasFontMetrics,
  onListingOpening,
  onListingProgress,
  onListingReadComplete,
  onListingComplete,
  onListingError,
  onListingCancelled,
} from './file-listing'
// Streaming-listing event payload types now flow from the typed-events bindings
// via the `file-listing.ts` re-export.
export type {
  ListingOpeningEvent,
  ListingProgressEvent,
  ListingReadCompleteEvent,
  ListingCompleteEvent,
  ListingErrorEvent,
  ListingCancelledEvent,
} from './file-listing'

// File viewer (session management, search, seeking)
export {
  viewerOpen,
  viewerOpenAsText,
  viewerGetLines,
  viewerSearchStart,
  viewerSearchPoll,
  viewerSearchCancel,
  viewerGetStatus,
  viewerClose,
  viewerSetupMenu,
  viewerSetWordWrap,
  viewerReadRange,
  viewerCancelRead,
  viewerWriteRangeToFile,
} from './file-viewer'
export type {
  LineChunk,
  BackendCapabilities,
  ViewerOpenResult,
  ViewerContentKind,
  MediaDimensions,
  ViewerSessionStatus,
  ViewerSearchMatch,
  ViewerSearchMode,
  ViewerSearchStatus,
  SearchPollResult,
  RangeEnd,
  ViewerError,
} from './file-viewer'

// File actions (open, reveal, preview, context menu)
export {
  openFile,
  openExternalUrl,
  showFileContextMenu,
  showBreadcrumbContextMenu,
  showVolumeRowContextMenu,
  showParentRowContextMenu,
  showInFinder,
  copyToClipboard,
  quickLookOpen,
  quickLookSetPath,
  quickLookClose,
  getInfo,
  openInEditor,
  cloudMakeAvailableOffline,
  cloudRemoveDownload,
} from './file-actions'

// Favorites (user-editable switcher favorites)
export { addFavorite, removeFavorite, renameFavorite, reorderFavorites, stripFavoritePrefix } from './favorites'

// Icons (fetching and cache management)
export {
  getIcons,
  getCustomFolderIconIds,
  refreshDirectoryIcons,
  clearExtensionIconCache,
  clearDirectoryIconCache,
} from './icons'

// App state (MCP pane state, dialog tracking, menu context, window lifecycle)
export {
  updateLeftPaneState,
  updateRightPaneState,
  updateFocusedPane,
  updatePaneTabs,
  notifyDialogOpened,
  notifyDialogClosed,
  registerKnownDialogs,
  updateMenuContext,
  activateWindowMenu,
  toggleHiddenFiles,
  syncMenuShowHidden,
  updateViewModeMenu,
  showMainWindow,
  orderWindowToBack,
  updatePinTabMenu,
  setReopenClosedTabEnabled,
} from './app-state'
export type { PaneFileEntry, PaneState, McpTabInfo } from './app-state'

// Shared IPC types (timeout-aware wrappers)
export type { TimedOut, IpcError } from './ipc-types'
export { isIpcError, getIpcErrorMessage, throwIpcError } from './ipc-types'

// Storage (volumes, space, permissions)
export {
  DEFAULT_VOLUME_ID,
  listVolumes,
  refreshVolumes,
  getDefaultVolumeId,
  resolvePathVolume,
  resolveLocation,
  getVolumeSpace,
  ejectVolume,
  getBusyVolumeIds,
  onVolumesChanged,
  onVolumeUnmounted,
  onVolumesBusyChanged,
  onVolumeContextAction,
  watchVolumeSpace,
  unwatchVolumeSpace,
  onVolumeSpaceChanged,
  onLowDiskSpace,
  setDiskSpaceThreshold,
  setLowDiskSpaceConfig,
  checkFullDiskAccess,
  checkFullDiskAccessQuiet,
  getRestrictedPaths,
  getMacosMajorVersion,
  openPrivacySettings,
  openSystemSettingsUrl,
  openAppearanceSettings,
} from './storage'
export type { Location, PathVolumeResolution, ResolveLocationResult, VolumeSpaceInfo } from './storage'

// Networking (SMB, keychain, mounting)
export {
  listNetworkHosts,
  getNetworkDiscoveryState,
  resolveNetworkHost,
  listSharesOnHost,
  prefetchShares,
  getKnownShareByName,
  updateKnownShare,
  getUsernameHints,
  saveSmbCredentials,
  getSmbCredentials,
  deleteSmbCredentials,
  isUsingCredentialFileFallback,
  listSharesWithCredentials,
  mountNetworkShare,
  upgradeToSmbVolume,
  upgradeToSmbVolumeWithCredentials,
  systemHasSavedSmbPassword,
  upgradeToSmbVolumeUsingSavedPassword,
  reconnectSmbVolume,
  reconnectSmbVolumeWithCredentials,
  disconnectSmbVolume,
  type UpgradeResult,
  connectToServer,
  removeManualServer,
  showNetworkHostContextMenu,
  onNetworkHostContextAction,
  onNetworkHostFound,
  onNetworkHostLost,
  onNetworkHostResolved,
  onNetworkDiscoveryStateChanged,
  onSmbConnectionChanged,
  disconnectNetworkHost,
  ensureNetworkDiscoveryStarted,
  setNetworkEnabled,
} from './networking'

// Git browser events
export { onGitStateChanged } from './git'

// Write operations (copy, move, delete)
export {
  listen,
  startScanPreview,
  cancelScanPreview,
  checkScanPreviewStatus,
  onScanPreviewProgress,
  onScanPreviewComplete,
  onScanPreviewError,
  onScanPreviewCancelled,
  copyFiles,
  moveFiles,
  deleteFiles,
  trashFiles,
  cancelWriteOperation,
  cancelAllWriteOperations,
  resolveWriteConflict,
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  onWriteSourceItemDone,
  formatBytes,
  formatDuration,
  formatFilesPerSecond,
} from './write-operations'
export type { Event, UnlistenFn } from './write-operations'

// Operation manager (queue window): list + pause/resume/cancel + the thin
// `operations-changed` membership/status event.
export {
  listOperations,
  cancelOperation,
  cancelOperations,
  pauseOperation,
  resumeOperation,
  pauseAll,
  resumeAll,
  onOperationsChanged,
} from './operations'
export type { OperationSnapshot, OperationsChanged } from './operations'

// Network types
export type { ManualConnectResult } from './networking'

export type { StreamingListingStartResult } from '../file-explorer/types'

// Write + scan-preview event payload types now flow from the typed-events
// bindings via the `write-operations.ts` re-export.
export type {
  WriteCancelledEvent,
  WriteCompleteEvent,
  WriteConflictEvent,
  WriteErrorEvent,
  WriteOperationConfig,
  WriteOperationError,
  WriteOperationStartResult,
  WriteProgressEvent,
  WriteSettledEvent,
  WriteSourceItemDoneEvent,
  ConflictInfo,
  DryRunResult,
  OperationStatus,
  OperationSummary,
  ScanProgressEvent,
  ScanPreviewStartResult,
  ScanPreviewProgressEvent,
  ScanPreviewCompleteEvent,
  ScanPreviewErrorEvent,
  ScanPreviewCancelledEvent,
} from './write-operations'

// Analytics (PostHog feature events through the single backend path)
export { trackEvent } from './analytics'

// Beta-tester signup (subscribes the contact email; sends NO install id)
export { betaSignup } from './beta-signup'
export type { BetaSignupResult } from './beta-signup'
export { sendFeedback } from './feedback'
export type { SendFeedbackResult } from './feedback'

// What's new popup
export { getWhatsNew, whatsNewDevOverride } from './whats-new'
export type { WhatsNewRelease, WhatsNewSection } from './whats-new'

// Crash reporter
export { checkPendingCrashReport, dismissCrashReport, sendCrashReport } from './crash-reporter'
export type { CrashReport } from './crash-reporter'

// Error reporter (Flow A: user-initiated; Flow B: auto-send event)
export {
  prepareErrorReportPreview,
  sendErrorReport,
  saveErrorReportToDisk,
  onErrorReportAutoSent,
} from './error-reporter'
export type { PreviewPayload, BundleManifest, ActiveSettingsSnapshot } from './error-reporter'

// AI lifecycle events
export {
  onAiDownloadProgress,
  onAiStarting,
  onAiServerReady,
  onAiVerifying,
  onAiInstalling,
  onAiInstallComplete,
  onAiExtracting,
} from './ai'

// Appearance / system events
export { onAccentColorChanged, onReduceTransparencyChanged, onSystemTextSizeChanged } from './appearance'

// Native-menu events
export { onViewModeChanged, onMenuSort } from './menu-events'

// Directory-watcher events
export { onDirectoryDiff, onDirectoryDeleted } from './directory-watcher'

// Native drag events
export { onDragImageSize, onDragModifiers, onDragOutSessionStarted, onDragOutSessionComplete } from './native-drag'

// Quick Look events
export { onQuickLookKey, onQuickLookClosed } from './quick-look'

// Downloads events
export { onDownloadDetected, onGlobalShortcutFired } from './downloads'

// Restricted-paths event
export { onRestrictedPathsChanged } from './restricted-paths'

// Window-management events (MCP dialog lifecycle, execute-command relay,
// settings self-close, viewer word-wrap, restricted-settings forward)
export {
  onExecuteCommand,
  emitExecuteCommand,
  onOpenSettings,
  requestOpenSettings,
  onOpenFileViewer,
  onFocusSettings,
  onFocusFileViewer,
  onFocusAbout,
  onFocusConfirmation,
  onCloseFileViewer,
  onCloseAllFileViewers,
  onCloseAbout,
  onCloseConfirmation,
  onMcpSettingsClose,
  onViewerWordWrapToggled,
  onPersistRestrictedSetting,
} from './dialog-events'

// Licensing
export {
  getLicenseStatus,
  getWindowTitle,
  activateLicense,
  verifyLicense,
  commitLicense,
  getLicenseInfo,
  markExpirationModalShown,
  markCommercialReminderDismissed,
  resetLicense,
  needsLicenseValidation,
  hasLicenseBeenValidated,
  validateLicenseWithServer,
  parseActivationError,
} from './licensing'
export type {
  LicenseType,
  LicenseStatus,
  LicenseInfo,
  VerifyResult,
  LicenseActivationErrorCode,
  LicenseActivationError,
} from './licensing'

// MTP (Android device support)
export {
  setMtpEnabled,
  getMtpDeviceDisplayName,
  listMtpDevices,
  isMtpConnectionError,
  connectMtpDevice,
  disconnectMtpDevice,
  getMtpDeviceInfo,
  getPtpcameradWorkaroundCommand,
  getMtpStorages,
  onMtpExclusiveAccessError,
  onMtpPermissionError,
  onMtpDeviceConnected,
  onMtpDeviceDisconnected,
  listMtpDirectory,
  downloadMtpFile,
  uploadToMtp,
  deleteMtpObject,
  createMtpFolder,
  renameMtpObject,
  moveMtpObject,
  onMtpTransferProgress,
  scanMtpForCopy,
  copyBetweenVolumes,
  moveBetweenVolumes,
  scanVolumeForCopy,
  scanVolumeForConflicts,
} from './mtp'
export type {
  MtpDeviceInfo,
  MtpStorageInfo,
  ConnectedMtpDeviceInfo,
  MtpConnectionError,
  MtpExclusiveAccessErrorEvent,
  MtpPermissionErrorEvent,
  MtpDeviceConnectedEvent,
  MtpDeviceDisconnectedEvent,
  MtpOperationResult,
  MtpObjectInfo,
  MtpTransferProgress,
  MtpScanResult,
  VolumeSpaceInfoExtended,
  VolumeConflictInfo,
  VolumeCopyScanResult,
  VolumeCopyConfig,
  SourceItemInput,
} from './mtp'

// Rename
export { checkRenamePermission, checkRenameValidity, moveToTrash, renameFile } from './rename'
export type { RenameConflictFileInfo, RenameValidityResult } from './rename'

// Settings and AI
export {
  checkPortAvailable,
  findAvailablePort,
  setMcpEnabled,
  setMcpPort,
  getMcpRunning,
  getMcpPort,
  updateFileWatcherDebounce,
  updateServiceResolveTimeout,
  setDirectSmbConnection,
  setFilterSafeSaveArtifacts,
  setSmbConcurrency,
  setMaxLogStorageMb,
  setErrorReportsEnabled,
  setShowVirtualGitPortal,
  setIndexingEnabled,
  startIndexingAfterFdaDecision,
  getDirStatsBatch,
  getE2eStartPath,
  isE2eMode,
  isForceOnboarding,
  getAiStatus,
  getAiModelInfo,
  startAiDownload,
  cancelAiDownload,
  uninstallAi,
  getFolderSuggestions,
  streamFolderSuggestions,
  getAiRuntimeStatus,
  configureAi,
  stopAiServer,
  startAiServer,
  checkAiConnection,
  saveAiApiKey,
  getAiApiKey,
  deleteAiApiKey,
  hasAiApiKey,
  getSystemMemoryInfo,
} from './settings'
export type {
  AiStatus,
  AiDownloadProgress,
  AiModelInfo,
  AiRuntimeStatus,
  AiConnectionCheckResult,
  DirStats,
  SystemMemoryInfo,
  SuggestionStreamEvent,
  FolderSuggestionsStream,
  McpServerOutcome,
} from './settings'

// Tab context menu
export { showTabContextMenu, onTabContextAction } from './tab'

// Clipboard file operations (copy/cut/paste files via system clipboard)
export {
  copyFilesToClipboard,
  cutFilesToClipboard,
  copyPathsToClipboard,
  cutPathsToClipboard,
  readClipboardFiles,
  readClipboardText,
  clearClipboardCutState,
} from './clipboard-files'
export type { ClipboardReadResult } from './clipboard-files'

// Search (whole-drive file search)
export {
  prepareSearchIndex,
  searchFiles,
  releaseSearchIndex,
  translateSearchQuery,
  parseSearchScope,
  getSystemDirExcludes,
  onSearchIndexReady,
  getRecentSearches,
  addRecentSearch,
  removeRecentSearch,
  clearRecentSearches,
  applyRecentSearchesMaxCount,
} from './search'
export type { PatternType, SearchResult, SearchResultEntry, PrepareResult, ParsedScope } from './ipc-types'
export type {
  SearchQuery,
  TranslateResult,
  TranslatedQuery,
  TranslateDisplay,
  HistoryEntry,
  HistoryFilters,
  HistoryMode,
} from '$lib/ipc/bindings'
export {
  translateSelectionQuery,
  getRecentSelections,
  addRecentSelection,
  removeRecentSelection,
  clearRecentSelections,
  applyRecentSelectionsMaxCount,
} from './selection'
export type { SelectionHistoryEntry, SelectionTranslateResult } from '$lib/ipc/bindings'

// Drive-indexing event listeners
export {
  onIndexScanStarted,
  onIndexScanProgress,
  onIndexScanComplete,
  onIndexScanAborted,
  onIndexAggregationProgress,
  onIndexAggregationComplete,
  onIndexRescanNotification,
  onIndexReplayProgress,
  onIndexReplayComplete,
  onIndexDirUpdated,
  onIndexMemoryWarning,
} from './indexing'
export type {
  IndexScanStartedEvent,
  IndexScanProgressEvent,
  IndexScanCompleteEvent,
  AggregationProgressEvent,
  IndexRescanNotificationEvent,
  IndexReplayProgressEvent,
  IndexReplayCompleteEvent,
  IndexDirUpdatedEvent,
  IndexMemoryWarningEvent,
} from '$lib/ipc/bindings'
