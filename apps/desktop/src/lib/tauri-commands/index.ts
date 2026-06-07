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
} from './file-listing'

// File viewer (session management, search, seeking)
export {
  viewerOpen,
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
  setMenuContext,
  toggleHiddenFiles,
  syncMenuShowHidden,
  updateViewModeMenu,
  showMainWindow,
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
  getVolumeSpace,
  ejectVolume,
  getBusyVolumeIds,
  onVolumeContextAction,
  watchVolumeSpace,
  unwatchVolumeSpace,
  setDiskSpaceThreshold,
  setLowDiskSpaceConfig,
  checkFullDiskAccess,
  getRestrictedPaths,
  getMacosMajorVersion,
  openPrivacySettings,
  openSystemSettingsUrl,
  openAppearanceSettings,
} from './storage'
export type { PathVolumeResolution, VolumeSpaceInfo } from './storage'

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
  disconnectNetworkHost,
  ensureNetworkDiscoveryStarted,
  setNetworkEnabled,
} from './networking'

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
  formatBytes,
  formatDuration,
  formatFilesPerSecond,
} from './write-operations'
export type { Event, UnlistenFn } from './write-operations'

// Network types
export type { ManualConnectResult } from './networking'

// Re-export types from write-operations (originally from file-explorer/types)
export type {
  ListingProgressEvent,
  ListingCompleteEvent,
  ListingErrorEvent,
  ListingCancelledEvent,
  StreamingListingStartResult,
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
} from '../file-explorer/types'

// Crash reporter
export { checkPendingCrashReport, dismissCrashReport, sendCrashReport } from './crash-reporter'
export type { CrashReport } from './crash-reporter'

// Error reporter (Flow A: user-initiated)
export { prepareErrorReportPreview, sendErrorReport, saveErrorReportToDisk } from './error-reporter'
export type { PreviewPayload, BundleManifest, ActiveSettingsSnapshot } from './error-reporter'

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
  optInAi,
  isAiOptedOut,
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
