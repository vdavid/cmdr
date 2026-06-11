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
    import { initVolumeTints, cleanupVolumeTints } from '$lib/file-explorer/pane/volume-tint.svelte'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { logWebkitCompat } from '$lib/utils/webkit-compat'
    import { initFocusWatchdog } from '$lib/focus-watchdog'
    import { initTextSize, cleanupTextSize } from '$lib/text-size.svelte'
    import { initializeShortcuts, setupMcpShortcutsListener, cleanupMcpShortcutsListener } from '$lib/shortcuts'
    import {
        setupMcpMainBridge,
        cleanupMcpMainBridge,
        setupRestrictedSettingsBridge,
        cleanupRestrictedSettingsBridge,
    } from '$lib/settings'
    import {
        onMtpExclusiveAccessError,
        onMtpPermissionError,
        onMtpDeviceConnected,
        connectMtpDevice,
        cancelAllWriteOperations,
        checkPendingCrashReport,
        sendCrashReport,
        type MtpExclusiveAccessErrorEvent,
        type MtpPermissionErrorEvent,
        type CrashReport,
    } from '$lib/tauri-commands'
    import { getSetting } from '$lib/settings'
    import { migrateApiKeysFromSettings, pushConfigToBackend } from '$lib/settings/ai-config'
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
    import ErrorReportDialog from '$lib/error-reporter/ErrorReportDialog.svelte'
    import { errorReportFlow } from '$lib/error-reporter/error-report-flow.svelte'
    import FeedbackDialog from '$lib/feedback/FeedbackDialog.svelte'
    import { feedbackFlow } from '$lib/feedback/feedback-flow.svelte'
    import {
        initAutoSendToastListener,
        cleanupAutoSendToastListener,
    } from '$lib/error-reporter/auto-send-toast.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { Snippet } from 'svelte'

    const crashLog = getAppLogger('crashReporter')

    interface Props {
        children?: Snippet
    }

    const { children }: Props = $props()

    // Gates the children render until settings are loaded and applied. File-explorer
    // components (FilePane, BriefList, ...) read getSetting() synchronously at mount; if
    // they mount before the store finishes loading they get the registry default, which
    // logs a pre-init-read warning and, worse, can push a default back to the backend as
    // if the user chose it (this is how a pre-init read of `ai.provider` could quietly set
    // AI to "off"). Mounting the page only once settings are ready closes that race for the
    // whole subtree. See settings-store.ts § getSetting and (main)/CLAUDE.md § Gotchas.
    let settingsReady = $state(false)

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
        ptpcameradBlockingProcess = event.blockingProcess ?? undefined
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

        // Catch focus leaks: if neither pane is keyboard-focused for 500 ms+
        // while the main window is active and no dialog is open, log a WARN
        // with the offending activeElement so we can trace the culprit.
        initFocusWatchdog()

        // Initialize all async setup
        void (async () => {
            try {
                // Initialize reactive settings for UI components
                await initReactiveSettings()

                // Initialize settings and apply them to CSS variables
                await initSettingsApplier()

                // Subscribe to volume-tint settings so FilePane bg updates live
                initVolumeTints()
            } finally {
                // Settings are now loaded, applied to CSS, and volume tints are wired, so the
                // file-explorer subtree can mount without any pre-init getSetting() reads (and
                // without a flash of default git chip / volume tint). In `finally` so a settings
                // load failure (which logs its own error in initializeSettings) still mounts the
                // page on registry defaults rather than leaving a blank window. Everything below
                // is independent of the children mounting, so it keeps running in the background.
                settingsReady = true
            }

            // Log once whether this WebKit supports the modern CSS we lean on
            // (`color-mix()`). Old Safari versions on macOS 12 Monterey fall
            // back to the static declarations in `app.css`; surfacing this in
            // logs lets us spot affected users in error reports without
            // depending on UA-string sniffing.
            logWebkitCompat()

            // One-time migration of pre-launch testers' plaintext API keys from settings.json to
            // the OS secret store. TODO: remove this call after 2026-09-01 (see function comment).
            // Awaited so the config push below reads the freshly-migrated key from the secret store.
            await migrateApiKeysFromSettings()

            // Push AI config to the backend (triggers server start if provider is local + model
            // installed). Goes through the single canonical `pushConfigToBackend()` — the same
            // read-fresh pusher the settings-applier and onboarding use — so there's ONE place that
            // reads `ai.provider` for the backend. Settings are already loaded by this point
            // (initReactiveSettings → initializeSettings above), so the read returns real values.
            void pushConfigToBackend()

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            // Apply compounded text size (system Accessibility × user setting)
            await initTextSize()

            // Initialize keyboard shortcuts store (loads custom shortcuts from disk)
            await initializeShortcuts()

            // Set up MCP shortcuts listener (allows MCP tools to modify shortcuts)
            await setupMcpShortcutsListener()

            // Set up MCP settings bridge (allows MCP tools to query/modify settings)
            await setupMcpMainBridge()

            // Set up the restricted-settings bridge (persists viewer-originated
            // setting changes; the viewer window has no store capability)
            await setupRestrictedSettingsBridge()

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

            // Listen for Flow B auto-send events so we can show the confirmation toast.
            // Mounted alongside the Flow A `ErrorReportDialog` for symmetry.
            void initAutoSendToastListener()

            // Start checking for updates
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
        cleanupTextSize()
        cleanupReactiveSettings()
        cleanupSettingsApplier()
        cleanupVolumeTints()
        cleanupMcpShortcutsListener()
        cleanupMcpMainBridge()
        cleanupRestrictedSettingsBridge()
        cleanupAutoSendToastListener()
    })
</script>

<ToastContainer />
{#if showCrashReportDialog && pendingCrashReport}
    <CrashReportDialog report={pendingCrashReport} onClose={closeCrashReportDialog} />
{/if}
{#if errorReportFlow.open}
    <ErrorReportDialog />
{/if}
{#if feedbackFlow.open}
    <FeedbackDialog />
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
    {#if settingsReady}
        {@render children?.()}
    {/if}
</div>

<style>
    .page-wrapper {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        /* Opaque main-window backdrop. The shared `app.html` keeps `html`
           and `body` transparent (so the settings window's translucent
           backdrop can show the macOS `NSVisualEffectView` behind it).
           Painting on `.page-wrapper` covers the whole main window
           without affecting any other window. */
        background-color: var(--color-bg-primary);
    }
</style>
