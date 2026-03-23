// Re-export all modules for backward compatibility
// This allows existing imports from '$lib/tauri-commands' to continue working

// File listing (on-demand virtual scrolling API, sync status, font metrics)
export {
    listDirectoryStart,
    cancelListing,
    resortListing,
    getFileRange,
    getTotalCount,
    getMaxFilenameWidth,
    findFileIndex,
    findFileIndices,
    getFileAt,
    listDirectoryEnd,
    refreshListing,
    getListingStats,
    refreshListingIndexSizes,
    startSelectionDrag,
    prepareSelfDragOverlay,
    clearSelfDragOverlay,
    getPathLimits,
    pathExists,
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
} from './file-viewer'
export type {
    LineChunk,
    BackendCapabilities,
    ViewerOpenResult,
    ViewerSessionStatus,
    ViewerSearchMatch,
    SearchPollResult,
} from './file-viewer'

// File actions (open, reveal, preview, context menu)
export {
    openFile,
    openExternalUrl,
    showFileContextMenu,
    showInFinder,
    copyToClipboard,
    quickLook,
    getInfo,
    openInEditor,
} from './file-actions'

// Icons (fetching and cache management)
export { getIcons, refreshDirectoryIcons, clearExtensionIconCache, clearDirectoryIconCache } from './icons'

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
    setViewMode,
    showMainWindow,
    updatePinTabMenu,
} from './app-state'
export type { PaneFileEntry, PaneState, McpTabInfo } from './app-state'

// Shared IPC types (timeout-aware wrappers)
export type { TimedOut, IpcError } from './ipc-types'
export { isIpcError, getIpcErrorMessage } from './ipc-types'

// Storage (volumes, space, permissions)
export {
    DEFAULT_VOLUME_ID,
    listVolumes,
    getDefaultVolumeId,
    findContainingVolume,
    getVolumeSpace,
    checkFullDiskAccess,
    openPrivacySettings,
    openAppearanceSettings,
} from './storage'
export type { VolumeSpaceInfo } from './storage'

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
} from './networking'

// Write operations (copy, move, delete)
export {
    listen,
    startScanPreview,
    cancelScanPreview,
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
    onWriteConflict,
    formatBytes,
    formatDuration,
} from './write-operations'
export type { Event, UnlistenFn } from './write-operations'

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
    getMtpDeviceDisplayName,
    listMtpDevices,
    isMtpConnectionError,
    connectMtpDevice,
    disconnectMtpDevice,
    getMtpDeviceInfo,
    getPtpcameradWorkaroundCommand,
    getMtpStorages,
    onMtpDeviceDetected,
    onMtpDeviceRemoved,
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
    scanVolumeForCopy,
    scanVolumeForConflicts,
} from './mtp'
export type {
    MtpDeviceInfo,
    MtpStorageInfo,
    ConnectedMtpDeviceInfo,
    MtpConnectionError,
    MtpDeviceDetectedEvent,
    MtpDeviceRemovedEvent,
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
    setIndexingEnabled,
    getDirStatsBatch,
    getE2eStartPath,
    getAiStatus,
    getAiModelInfo,
    startAiDownload,
    cancelAiDownload,
    dismissAiOffer,
    uninstallAi,
    optOutAi,
    optInAi,
    isAiOptedOut,
    getFolderSuggestions,
    getAiRuntimeStatus,
    configureAi,
    stopAiServer,
    startAiServer,
    checkAiConnection,
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
} from './settings'

// Tab context menu
export { showTabContextMenu, onTabContextAction } from './tab'

// Clipboard file operations (copy/cut/paste files via system clipboard)
export {
    copyFilesToClipboard,
    cutFilesToClipboard,
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
} from './search'
export type {
    PatternType,
    SearchQuery,
    SearchResult,
    SearchResultEntry,
    PrepareResult,
    TranslateResult,
    TranslatedQuery,
    TranslateDisplay,
    ParsedScope,
} from './ipc-types'
