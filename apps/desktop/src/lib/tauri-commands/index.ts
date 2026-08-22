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
  getFileBeside,
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
  extendFontMetrics,
  hasFontMetrics,
  onListingOpening,
  onListingProgress,
  onListingReadComplete,
  onListingComplete,
  onListingError,
  onListingCancelled,
  getBriefColumnTextWidths,
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
  viewerSetEncoding,
  viewerSetTailMode,
  viewerReload,
  viewerGetEncodingOptions,
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
  setFileOperationsBlocked,
  updateMenuContext,
  activateWindowMenu,
  syncMenuShowHidden,
  updateViewModeMenu,
  showMainWindow,
  orderWindowToBack,
  updatePinTabMenu,
  setReopenClosedTabEnabled,
  getChildWindowRect,
  setChildWindowRect,
  updateMenuAccelerator,
  setUiLanguage,
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
  onVolumeConnectionChanged,
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
  onSmbFellBackToOsMount,
  disconnectNetworkHost,
  ensureNetworkDiscoveryStarted,
  setNetworkEnabled,
} from './networking'

// Git browser commands and events
export { getGitRepoInfo, subscribeGitState, unsubscribeGitState, getGitStatusForPaths, onGitStateChanged } from './git'

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
  resolveWriteConflict,
  onWriteProgress,
  onWriteComplete,
  onWriteError,
  onWriteCancelled,
  onWriteSettled,
  onWriteConflict,
  onWriteConflictResolved,
  onWriteSourceItemDone,
} from './write-operations'
export type { Event, UnlistenFn } from './write-operations'

// Operation manager (queue window): list + pause/resume/cancel + dismissing a
// retained failure + the thin `operations-changed` membership/status event.
export {
  listOperations,
  cancelOperation,
  cancelOperations,
  pauseOperation,
  resumeOperation,
  pauseAll,
  resumeAll,
  dismissFailedOperation,
  dismissAllFailedOperations,
  onOperationsChanged,
} from './operations'
export type { OperationSnapshot, OperationsChanged } from './operations'

// The quit gate: the backend holding an exit while operations run, and the
// dialog's two answers.
export { quitConfirm, quitCancel, onQuitRequested } from './quit'
export type { QuitRequested } from './quit'

// Network types
export type { ManualConnectResult } from './networking'

export type { StreamingListingStartResult } from '../file-explorer/types'

/** Which side of a named row `getFileBeside` reads. */
export type { RowBeside } from '$lib/ipc/bindings'

// Write + scan-preview event payload types now flow from the typed-events
// bindings via the `write-operations.ts` re-export.
export type {
  TransferActivity,
  TransferWaitReason,
  WriteCancelledEvent,
  WriteCompleteEvent,
  WriteConflictEvent,
  WriteConflictResolvedEvent,
  WriteErrorEvent,
  WriteOperationConfig,
  WriteOperationError,
  WriteOperationStartResult,
  WriteProgressEvent,
  WriteSettledEvent,
  WriteSourceItemDoneEvent,
  ConflictId,
  ConflictInfo,
  ConflictResolutionOutcome,
  DryRunResult,
  Initiator,
  OperationStatus,
  OperationSummary,
  ScanProgressEvent,
  ScanPreviewStartResult,
  ScanPreviewProgressEvent,
  ScanPreviewCompleteEvent,
  ScanPreviewErrorEvent,
  ScanPreviewCancelledEvent,
  CompressedSizeEstimate,
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

export { getRecentOperationLogEntries, getOperationLogDetail, undoOperations } from './operation-log'
export type {
  OperationRow,
  OperationItemView,
  OperationLogDetail,
  OperationUndoOutcome,
  SkipBreakdown,
  SkipReason,
  UndoReport,
} from './operation-log'

// Ask Cmdr chat rail
export {
  sendAskCmdrMessage,
  cancelAskCmdr,
  applyBulkRename,
  preflightBulkRename,
  cancelBulkRenameProposal,
  reviseBulkRenameRow,
  recordAskCmdrModelChange,
  getAskCmdrConversation,
  listAskCmdrConversations,
  searchAskCmdrConversations,
  renameAskCmdrConversation,
  archiveAskCmdrConversation,
  askCmdrSelectionAttachments,
  resolveAskCmdrAttachments,
  askCmdrFakeActive,
  askCmdrConsentStatus,
  acceptAskCmdrConsent,
  revokeAskCmdrConsent,
  askCmdrConversationCost,
  askCmdrCostSummary,
  askCmdrModelWindow,
} from './ask-cmdr'
export type {
  AskCmdrStreamEvent,
  AskCmdrErrorKind,
  AskCmdrUsage,
  StopReason,
  ConversationRow,
  ConversationDetailView,
  ConversationSearchHit,
  MessageView,
  MessageBlock,
  AttachmentRef,
  AttachmentKindView,
  AskCmdrConsentStatus,
  ConversationCost,
  CostSummary,
  ModelWindowView,
  RenameEvidence,
  RenameEvidenceCoverage,
  RenameEvidenceSource,
  RenameProposalRow,
} from './ask-cmdr'

// Suggested ops (the review dialog's reads, and the rejection it records)
export {
  approveSuggestedGroup,
  listSuggestedOps,
  onSuggestionsChanged,
  pageSuggestedOps,
  rejectSuggestedGroup,
} from './suggested-ops'
export type {
  ApprovalResultView,
  DestinationState,
  RejectResultView,
  SuggestionChange,
  SuggestionsChanged,
  SuggestedGroupView,
  SuggestedOpPage,
  SuggestedOpView,
  SuggestedSweepView,
} from './suggested-ops'

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

// Appearance / system-environment (accent color, reduce-transparency, text-size, localized strings)
export {
  getAccentColor,
  getShouldReduceTransparency,
  getSystemTextSizeMultiplier,
  getLocalizedSystemStrings,
  getOsLocales,
  onAccentColorChanged,
  onReduceTransparencyChanged,
  onSystemTextSizeChanged,
  onOsLocalesChanged,
} from './appearance'

// Native-menu events
export {
  onViewModeChanged,
  onMenuSort,
  onMediaIndexFolderExclusion,
  onMediaIndexFolderChoice,
  onMenuBarRebuilt,
} from './menu-events'

// Directory-watcher events
export { onDirectoryDiff, onDirectoryDeleted } from './directory-watcher'

// Native drag events
export { onDragImageSize, onDragModifiers, onDragOutSessionStarted, onDragOutSessionComplete } from './native-drag'

// Quick Look events
export { onQuickLookKey, onQuickLookClosed } from './quick-look'

// Downloads commands and events
export {
  downloadsWatcherStatus,
  goToLatestDownload,
  setGlobalGoToLatestShortcut,
  recheckDownloadsWatcherGate,
  onDownloadDetected,
  onGlobalShortcutFired,
} from './downloads'

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
  requestForegroundOperation,
  onForegroundOperationRequested,
} from './dialog-events'

// Licensing
export {
  getLicenseStatus,
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
  deleteMtpObject,
  createMtpFolder,
  renameMtpObject,
  moveMtpObject,
  scanMtpForCopy,
  copyBetweenVolumes,
  moveBetweenVolumes,
  compressFiles,
  scanVolumeForCopy,
  scanVolumeForConflicts,
} from './mtp'
// Archive-password commands (encrypted-archive unlock)
export { setArchivePassword, clearArchivePassword } from './archive'
export type {
  MtpDeviceInfo,
  MtpStorageInfo,
  ConnectedMtpDeviceInfo,
  MtpConnectionError,
  MtpExclusiveAccessErrorEvent,
  MtpPermissionErrorEvent,
  MtpDeviceConnectedEvent,
  MtpDeviceDisconnectedEvent,
  MtpObjectInfo,
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
  setShowSafeSaveFiles,
  setShowStagingTempFiles,
  setLogLlmCalls,
  setSmbConcurrency,
  setMaxLogStorageMb,
  setErrorReportsEnabled,
  setShowVirtualGitPortal,
  setIndexingEnabled,
  setImageIndexEnabled,
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
  getAiApiKeyStatus,
  deleteAiApiKey,
  getSystemMemoryInfo,
  getRestrictedWindowSettings,
  recordSettingsDefaults,
  onSettingsChanged,
  persistRestrictedWindowSetting,
} from './settings'
export type {
  AiStatus,
  AiDownloadProgress,
  AiModelInfo,
  AiRuntimeStatus,
  AiConnectionCheckResult,
  AiApiKeyStatus,
  ConfigureAiOutcome,
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
  pasteClipboardAsFile,
} from './clipboard-files'
export type { ClipboardReadResult, PastedClipboardFile } from './clipboard-files'

// Search (whole-drive file search)
export {
  prepareSearchIndex,
  searchFiles,
  searchFilesStreaming,
  cancelSearch,
  onSearchProgress,
  onSearchComplete,
  onSearchCancelled,
  onSearchError,
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
  LiveSearchStart,
  SearchProgressEvent,
  SearchCompleteEvent,
  SearchCancelledEvent,
  SearchErrorEvent,
  SearchRunCoverage,
  WalkEnding,
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

// Drive-indexing commands
export {
  getIndexStatus,
  getVolumeIndexStatusById,
  enableDriveIndex,
  disableDriveIndex,
  forgetDriveIndex,
  rescanDriveIndex,
  clearDriveIndex,
  getIndexDiskUsage,
  recordVisit,
} from './indexing'

// Media index (image-ML): OCR search, per-volume state, thumbnail tokens
export {
  mediaIndexSearchOcr,
  mediaIndexVolumeState,
  mediaIndexFileStatus,
  mediaIndexFolderCoverage,
  mediaIndexThumbnailToken,
  mediaIndexDropThumbnailTokens,
  mediaIndexSetNetworkVolumeEnabled,
  mediaIndexSetAlwaysIndexVolume,
  mediaIndexSetAlwaysIndexFolder,
  mediaIndexSetScope,
  mediaIndexSetExcludedFolder,
  setImageImportanceThreshold,
  setImageParallelism,
  getMediaIndexMaxParallelism,
  mediaIndexCoveredCount,
  mediaIndexReclaimPreview,
  mediaIndexPruneBelowThreshold,
  mediaIndexFindSimilar,
  mediaIndexSearchSemantic,
  mediaIndexSetSemanticSearchEnabled,
  mediaIndexClipModelStatus,
  mediaIndexDownloadClipModel,
  mediaIndexDeleteClipModel,
  onMediaEnrichProgress,
  onMediaEnrichTerminal,
} from './media-index'
export type {
  ClipModelStatus,
  CoveredCount,
  FileIndexState,
  FileIndexStatus,
  FolderCoverage,
  MediaEnrichProgressEvent,
  MediaEnrichTerminalEvent,
  MediaEnrichTerminalReason,
  MediaIndexVolumeState,
  OcrHit,
  ReclaimPreview,
  ReclaimResult,
  SemanticHit,
  SimilarImage,
} from './media-index'

// Drive-indexing event listeners
export {
  onIndexScanStarted,
  onIndexScanProgress,
  onIndexScanComplete,
  onIndexScanAborted,
  onIndexCoverageBranchStarted,
  onIndexCoverageBranchEnded,
  onIndexCoveragePhaseStarted,
  onIndexPhaseChanged,
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

// "Go to path" (⌘G): resolving typed input, and the persisted recents list
export { resolveGoToPath, getRecentPaths, addRecentPath, removeRecentPath } from './go-to-path'

// macOS Finder color tags
export { toggleTags, enrichTags } from './tags'

// macOS custom updater (check / download / install)
export { checkForUpdate, downloadUpdate, installUpdate } from './updates'
export type { UpdateCheckResult } from './updates'

// Dev/benchmark IPC
export { benchmarkLog } from './debug'
