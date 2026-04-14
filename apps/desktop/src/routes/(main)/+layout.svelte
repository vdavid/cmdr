<script lang="ts">
    /**
     * Main window layout - includes updater, notifications, window state, and settings.
     * Only used for the main file manager window.
     */
    import { onMount, onDestroy } from 'svelte'
    import { initWindowStateListener } from '$lib/window-state'
    import { startUpdateChecker } from '$lib/updates/updater.svelte'
    import { initSettingsApplier, cleanupSettingsApplier } from '$lib/settings/settings-applier'
    import { initReactiveSettings, cleanupReactiveSettings } from '$lib/settings/reactive-settings.svelte'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initializeShortcuts, setupMcpShortcutsListener, cleanupMcpShortcutsListener } from '$lib/shortcuts'
    import { setupMcpMainBridge, cleanupMcpMainBridge } from '$lib/settings'
    import {
        onMtpExclusiveAccessError,
        onMtpPermissionError,
        onMtpDeviceConnected,
        connectMtpDevice,
        cancelAllWriteOperations,
        configureAi,
        checkPendingCrashReport,
        sendCrashReport,
        type MtpExclusiveAccessErrorEvent,
        type MtpPermissionErrorEvent,
        type CrashReport,
    } from '$lib/tauri-commands'
    import { getSetting, resolveCloudConfig } from '$lib/settings'
    import { initAiState } from '$lib/ai/ai-state.svelte'
    import { initAiToastSync } from '$lib/ai/ai-toast-sync.svelte'
    import { addToast } from '$lib/ui/toast'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import { MtpPermissionDialog, PtpcameradDialog } from '$lib/mtp'
    import MtpConnectedToastContent, {
        setLastConnectedDeviceName,
    } from '$lib/mtp/MtpConnectedToastContent.svelte'
    import CrashReportDialog from '$lib/crash-reporter/CrashReportDialog.svelte'
    import CrashReportToastContent from '$lib/crash-reporter/CrashReportToastContent.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { Snippet } from 'svelte'

    const crashLog = getAppLogger('crashReporter')

    interface Props {
        children?: Snippet
    }

    const { children }: Props = $props()

    // State for crash report dialog
    let showCrashReportDialog = $state(false)
    let pendingCrashReport = $state<CrashReport | null>(null)

    // State for ptpcamerad dialog (macOS)
    let showPtpcameradDialog = $state(false)
    let ptpcameradBlockingProcess = $state<string | undefined>(undefined)
    let pendingDeviceId = $state<string | undefined>(undefined)

    // State for permission dialog (Linux)
    let showPermissionDialog = $state(false)
    let permissionPendingDeviceId = $state<string | undefined>(undefined)

    function handleMtpExclusiveAccessError(event: MtpExclusiveAccessErrorEvent) {
        ptpcameradBlockingProcess = event.blockingProcess
        pendingDeviceId = event.deviceId
        showPtpcameradDialog = true
    }

    function closePtpcameradDialog() {
        showPtpcameradDialog = false
        ptpcameradBlockingProcess = undefined
        pendingDeviceId = undefined
    }

    function handleMtpPermissionError(event: MtpPermissionErrorEvent) {
        permissionPendingDeviceId = event.deviceId
        showPermissionDialog = true
    }

    function closePermissionDialog() {
        showPermissionDialog = false
        permissionPendingDeviceId = undefined
    }

    async function retryPermissionConnection() {
        if (permissionPendingDeviceId) {
            const deviceId = permissionPendingDeviceId
            closePermissionDialog()
            try {
                await connectMtpDevice(deviceId)
            } catch {
                // Error will trigger another event if still permission denied
            }
        }
    }

    async function retryMtpConnection() {
        if (pendingDeviceId) {
            const deviceId = pendingDeviceId
            closePtpcameradDialog()
            try {
                await connectMtpDevice(deviceId)
            } catch {
                // Error will trigger another event if it's still exclusive access
            }
        }
    }

    function closeCrashReportDialog() {
        showCrashReportDialog = false
        pendingCrashReport = null
    }

    async function checkForPendingCrashReport() {
        try {
            const report = await checkPendingCrashReport()
            if (!report) return

            const autoSend = getSetting('updates.crashReports')

            if (autoSend && !report.possibleCrashLoop) {
                // Auto-send without dialog
                try {
                    await sendCrashReport(report)
                    addToast(CrashReportToastContent, {
                        id: 'crash-report-sent',
                        level: 'info',
                        dismissal: 'persistent',
                    })
                    crashLog.info('Crash report auto-sent')
                } catch (e) {
                    crashLog.warn('Auto-send crash report returned an error: {error}', {
                        error: String(e),
                    })
                }
            } else {
                // Show dialog for user to decide
                pendingCrashReport = report
                showCrashReportDialog = true
            }
        } catch (e) {
            crashLog.warn('Crash report check returned an error: {error}', { error: String(e) })
        }
    }

    // Cleanup functions stored for onDestroy
    let mtpExclusiveUnlistenPromise: Promise<() => void> | undefined
    let mtpPermissionUnlistenPromise: Promise<() => void> | undefined
    let mtpConnectedUnlistenPromise: Promise<() => void> | undefined
    let updateCleanup: (() => void) | undefined
    let aiCleanup: (() => void) | undefined

    onMount(() => {
        // Sync AI state to toast. Must be called synchronously (not after an await)
        // because it uses $effect, which requires Svelte's reactive context.
        initAiToastSync()

        // Initialize all async setup
        void (async () => {
            // Initialize reactive settings for UI components
            await initReactiveSettings()

            // Initialize settings and apply them to CSS variables
            await initSettingsApplier()

            // Push AI config to backend (triggers server start if provider is local + model installed)
            const resolvedConfig = resolveCloudConfig(
                getSetting('ai.cloudProvider'),
                getSetting('ai.cloudProviderConfigs'),
            )
            void configureAi(
                getSetting('ai.provider'),
                Number(getSetting('ai.localContextSize')),
                resolvedConfig.apiKey,
                resolvedConfig.baseUrl,
                resolvedConfig.model,
            )

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            // Initialize keyboard shortcuts store (loads custom shortcuts from disk)
            await initializeShortcuts()

            // Set up MCP shortcuts listener (allows MCP tools to modify shortcuts)
            await setupMcpShortcutsListener()

            // Set up MCP settings bridge (allows MCP tools to query/modify settings)
            await setupMcpMainBridge()

            // Initialize window state persistence on resize
            // This ensures window size/position survives hot reloads
            void initWindowStateListener()

            // Listen for MTP connection errors
            mtpExclusiveUnlistenPromise = onMtpExclusiveAccessError(handleMtpExclusiveAccessError)
            mtpPermissionUnlistenPromise = onMtpPermissionError(handleMtpPermissionError)

            // Listen for MTP device connections and show info toast
            mtpConnectedUnlistenPromise = onMtpDeviceConnected((event) => {
                if (!getSetting('fileOperations.mtpConnectionWarning')) return
                // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
                setLastConnectedDeviceName(event.deviceName || 'MTP device')
                addToast(MtpConnectedToastContent, {
                    id: 'mtp-connected',
                    dismissal: 'persistent',
                    level: 'info',
                })
            })

            // Check for pending crash reports from a previous session
            void checkForPendingCrashReport()

            // Start checking for updates (skips in dev mode)
            updateCleanup = startUpdateChecker()

            // Initialize AI state and event listeners (shows offer toast if eligible)
            aiCleanup = await initAiState()

            // Cancel all active write operations on page unload (hot-reload, close, navigation)
            window.addEventListener('beforeunload', () => {
                void cancelAllWriteOperations()
            })
        })()
    })

    onDestroy(() => {
        // Cleanup MTP listeners
        void mtpExclusiveUnlistenPromise?.then((unlisten) => {
            unlisten()
        })
        void mtpPermissionUnlistenPromise?.then((unlisten) => {
            unlisten()
        })
        void mtpConnectedUnlistenPromise?.then((unlisten) => {
            unlisten()
        })
        // Cleanup update checker
        updateCleanup?.()
        // Cleanup AI event listeners
        aiCleanup?.()
        // Cleanup other modules
        cleanupAccentColor()
        cleanupReactiveSettings()
        cleanupSettingsApplier()
        cleanupMcpShortcutsListener()
        cleanupMcpMainBridge()
    })
</script>

<ToastContainer />
{#if showCrashReportDialog && pendingCrashReport}
    <CrashReportDialog report={pendingCrashReport} onClose={closeCrashReportDialog} />
{/if}
{#if showPtpcameradDialog}
    <PtpcameradDialog
        blockingProcess={ptpcameradBlockingProcess}
        onClose={closePtpcameradDialog}
        onRetry={retryMtpConnection}
    />
{/if}
{#if showPermissionDialog}
    <MtpPermissionDialog onClose={closePermissionDialog} onRetry={retryPermissionConnection} />
{/if}
<div class="page-wrapper">
    {@render children?.()}
</div>

<style>
    .page-wrapper {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }
</style>
