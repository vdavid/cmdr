// Re-export all modules for backward compatibility
// This allows existing imports from '$lib/tauri-commands' to continue working

// File listing (on-demand virtual scrolling API)
export {
    listDirectoryStart,
    cancelListing,
    resortListing,
    getFileRange,
    getTotalCount,
    getMaxFilenameWidth,
    findFileIndex,
    getFileAt,
    listDirectoryEnd,
    getListingStats,
    startSelectionDrag,
    prepareSelfDragOverlay,
    clearSelfDragOverlay,
    pathExists,
    createDirectory,
} from './file-listing'

// File viewer
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
    openFile,
    openExternalUrl,
    getIcons,
    refreshDirectoryIcons,
    clearExtensionIconCache,
    showFileContextMenu,
    updateMenuContext,
    toggleHiddenFiles,
    setViewMode,
    updateLeftPaneState,
    updateRightPaneState,
    updateFocusedPane,
    notifyDialogOpened,
    notifyDialogClosed,
    registerKnownDialogs,
    showInFinder,
    copyToClipboard,
    quickLook,
    getInfo,
    openInEditor,
    showMainWindow,
    getSyncStatus,
    storeFontMetrics,
    hasFontMetrics,
} from './file-viewer'
export type {
    LineChunk,
    BackendCapabilities,
    ViewerOpenResult,
    ViewerSessionStatus,
    ViewerSearchMatch,
    SearchPollResult,
    PaneFileEntry,
    PaneState,
} from './file-viewer'

// Storage (volumes, space, permissions)
export {
    DEFAULT_VOLUME_ID,
    listVolumes,
    getDefaultVolumeId,
    findContainingVolume,
    getVolumeSpace,
    checkFullDiskAccess,
    openPrivacySettings,
} from './storage'
export type { VolumeSpaceInfo } from './storage'

// Networking (SMB, keychain, mounting)
export {
    listNetworkHosts,
    getNetworkDiscoveryState,
    resolveNetworkHost,
    listSharesOnHost,
    prefetchShares,
    feLog,
    getKnownShareByName,
    updateKnownShare,
    getUsernameHints,
    saveSmbCredentials,
    getSmbCredentials,
    deleteSmbCredentials,
    listSharesWithCredentials,
    isKeychainError,
    mountNetworkShare,
    isMountError,
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
    cancelWriteOperation,
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

// Licensing
export {
    getLicenseStatus,
    getWindowTitle,
    activateLicense,
    getLicenseInfo,
    markExpirationModalShown,
    markCommercialReminderDismissed,
    resetLicense,
    needsLicenseValidation,
    validateLicenseWithServer,
} from './licensing'
export type { LicenseType, LicenseStatus, LicenseInfo } from './licensing'

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
    updateFileWatcherDebounce,
    updateServiceResolveTimeout,
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
} from './settings'
export type { AiStatus, AiDownloadProgress, AiModelInfo } from './settings'
