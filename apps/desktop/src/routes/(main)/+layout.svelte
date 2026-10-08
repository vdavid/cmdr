<script lang="ts">
    /**
     * Main window layout - includes updater, notifications, window state, and settings.
     * Only used for the main file manager window.
     */
    import { onMount, onDestroy } from 'svelte'
    import {
        dismissMoveToApplicationsNudge,
        startUpdateChecker,
        updateBlockerNotice,
    } from '$lib/updates/updater.svelte'
    import MoveToApplicationsDialog from '$lib/updates/MoveToApplicationsDialog.svelte'
    import { initSettingsApplier, cleanupSettingsApplier } from '$lib/settings/settings-applier'
    import { initReactiveSettings, cleanupReactiveSettings } from '$lib/settings/reactive-settings.svelte'
    import { initVolumeTints, cleanupVolumeTints } from '$lib/file-explorer/pane/volume-tint.svelte'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initGlassMaterial, cleanupGlassMaterial } from '$lib/glass-material'
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
        type MtpExclusiveAccessErrorEvent,
        type MtpPermissionErrorEvent,
        type CrashReport,
    } from '$lib/tauri-commands'
    import { getSetting } from '$lib/settings'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import { settleHeldCloudConsentRevoke } from '$lib/ai/cloud-consent.svelte'
    import { mapLegacyAskCmdrOptIn } from '$lib/ask-cmdr/ask-cmdr-enabled-mapping'
    import { runInitSteps } from './init-steps'
    import { initAiState } from '$lib/ai/ai-state.svelte'
    import { initAiToastSync } from '$lib/ai/ai-toast-sync.svelte'
    import { addToast } from '$lib/ui/toast'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import { MtpPermissionDialog, PtpcameradDialog } from '$lib/mtp'
    import MtpConnectedToastContent from '$lib/mtp/MtpConnectedToastContent.svelte'
    import CrashReportDialog from '$lib/crash-reporter/CrashReportDialog.svelte'
    import { checkForPendingCrashReport } from '$lib/crash-reporter/pending-crash-report'
    import ErrorReportDialog from '$lib/error-reporter/ErrorReportDialog.svelte'
    import { errorReportFlow } from '$lib/error-reporter/error-report-flow.svelte'
    import FeedbackDialog from '$lib/feedback/FeedbackDialog.svelte'
    import { feedbackFlow } from '$lib/feedback/feedback-flow.svelte'
    import SignInSheet from '$lib/servers/SignInSheet.svelte'
    import { closeSignInSheet, currentSignInRequest } from '$lib/servers/sign-in-sheet-state.svelte'
    import { initAutoSendToastListener, cleanupAutoSendToastListener } from '$lib/error-reporter/auto-send-toast.svelte'
    // Dialog gallery harness (Debug > Soft dialogs). Gated below on
    // `import.meta.env.DEV || __CMDR_E2E_BUILD__`, both of which Vite inlines to
    // build-time booleans, so the harness and every dialog it imports drop out of
    // production builds. The E2E flag is what lets the i18n screenshot driver and
    // the E2E lane open gallery states on their shared binary; see `dialog-gallery/DETAILS.md`.
    import DialogGallery from '$lib/dialog-gallery/DialogGallery.svelte'
    import QuitConfirmationDialog from '$lib/quit/QuitConfirmationDialog.svelte'
    import { quitPrompt, initQuitPrompt, cleanupQuitPrompt } from '$lib/quit/quit-prompt.svelte'
    import type { Snippet } from 'svelte'

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
    /** The one sign-in sheet's current request, or `null` when nothing is asking. */
    const signInRequest = $derived(currentSignInRequest())
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

    /** The dialog half of the next-launch crash check (`$lib/crash-reporter/pending-crash-report.ts`). */
    function showCrashReport(report: CrashReport) {
        pendingCrashReport = report
        showCrashReportDialog = true
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

        // Listen for the backend holding a quit. Synchronous and first: the
        // gate can raise the prompt at any moment, including while the rest of
        // this mount is still awaiting IPC, and a missed `quit-requested` means
        // the app quits on its own countdown with no dialog ever shown.
        initQuitPrompt()

        // Catch focus leaks: if neither pane is keyboard-focused for 500 ms+
        // while the main window is active and no dialog is open, log a WARN
        // with the offending activeElement so we can trace the culprit.
        initFocusWatchdog()

        // All async setup, as ordered steps. Each is awaited before the next and runs inside
        // its own catch (`init-steps.ts`), so one step that throws never skips the rest.
        void runInitSteps([
            {
                name: 'settings',
                run: async () => {
                    try {
                        // Initialize reactive settings for UI components
                        await initReactiveSettings()

                        // Initialize settings and apply them to CSS variables
                        await initSettingsApplier()

                        // Subscribe to volume-tint settings so FilePane bg updates live
                        initVolumeTints()
                    } finally {
                        // Settings are now loaded, applied to CSS, and volume tints are wired, so
                        // the file-explorer subtree can mount without any pre-init getSetting()
                        // reads (and without a flash of default git chip / volume tint). In
                        // `finally` so a settings load failure (which logs its own error in
                        // initializeSettings) still mounts the page on registry defaults rather
                        // than leaving a blank window. The steps below are independent of the
                        // children mounting, so they keep running in the background.
                        settingsReady = true
                    }
                },
            },
            {
                // Retry a "no" to cloud AI the store refused. It already holds (every cloud gate
                // reads the held "no"); this is what lets the store catch up without anyone
                // opening a gate.
                name: 'heldCloudConsentRevoke',
                run: settleHeldCloudConsentRevoke,
            },
            {
                // Set `askCmdr.enabled` once for an install that predates the switch, from the
                // old Ask Cmdr opt-in. Needs the settings loaded; a no-op once the switch is set.
                name: 'askCmdrEnabledMapping',
                run: mapLegacyAskCmdrOptIn,
            },
            {
                // Log once whether this WebKit supports the modern CSS we lean on
                // (`color-mix()`). Old Safari versions on macOS 12 Monterey fall
                // back to the static declarations in `app.css`; surfacing this in
                // logs lets us spot affected users in error reports without
                // depending on UA-string sniffing.
                name: 'webkitCompat',
                run: logWebkitCompat,
            },
            {
                // Push AI config to the backend (triggers server start if provider is local + model
                // installed). Goes through the single canonical `pushConfigToBackend()` — the same
                // read-fresh pusher the settings-applier and onboarding use — so there's ONE place
                // that reads `ai.provider` for the backend. Settings are already loaded by this point
                // (initReactiveSettings → initializeSettings above), so the read returns real values.
                name: 'aiConfigPush',
                run: () => {
                    void pushConfigToBackend()
                },
            },
            // Read system accent color from macOS and listen for changes
            { name: 'accentColor', run: initAccentColor },
            { name: 'glassMaterial', run: initGlassMaterial },
            {
                // Apply compounded text size (system Accessibility × user setting).
                // This is the window that renders Brief mode, so it's the one that
                // measures font metrics; the others opt out by default.
                name: 'textSize',
                run: () => initTextSize({ measuresFontMetrics: true }),
            },
            // Initialize keyboard shortcuts store (loads custom shortcuts from disk)
            { name: 'shortcuts', run: initializeShortcuts },
            // Set up MCP shortcuts listener (allows MCP tools to modify shortcuts)
            { name: 'mcpShortcutsListener', run: setupMcpShortcutsListener },
            // Set up MCP settings bridge (allows MCP tools to query/modify settings)
            { name: 'mcpMainBridge', run: setupMcpMainBridge },
            // Set up the restricted-settings bridge (persists viewer-originated
            // setting changes; the viewer window has no store capability)
            { name: 'restrictedSettingsBridge', run: setupRestrictedSettingsBridge },
            {
                name: 'mtpListeners',
                run: () => {
                    // Listen for MTP connection errors
                    mtpExclusiveUnlistenPromise = onMtpExclusiveAccessError(handleMtpExclusiveAccessError)
                    mtpPermissionUnlistenPromise = onMtpPermissionError(handleMtpPermissionError)

                    // Listen for MTP device connections and show info toast
                    mtpConnectedUnlistenPromise = onMtpDeviceConnected((event) => {
                        if (!getSetting('fileOperations.mtpConnectionWarning')) return
                        addToast(MtpConnectedToastContent, {
                            id: 'mtp-connected',
                            dismissal: 'persistent',
                            level: 'info',
                            props: { deviceName: event.deviceName },
                        })
                    })
                },
            },
            {
                // Check for pending crash reports from a previous session
                name: 'crashReportCheck',
                run: () => {
                    void checkForPendingCrashReport(showCrashReport)
                },
            },
            {
                // Listen for Flow B auto-send events so we can show the confirmation toast.
                // Mounted alongside the Flow A `ErrorReportDialog` for symmetry.
                name: 'autoSendToastListener',
                run: () => {
                    void initAutoSendToastListener()
                },
            },
            {
                // Start checking for updates
                name: 'updateChecker',
                run: () => {
                    updateCleanup = startUpdateChecker()
                },
            },
            {
                // Initialize AI state and event listeners (shows offer toast if eligible)
                name: 'aiState',
                run: async () => {
                    aiCleanup = await initAiState()
                },
            },
        ])
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
        cleanupGlassMaterial()
        cleanupTextSize()
        cleanupReactiveSettings()
        cleanupSettingsApplier()
        cleanupVolumeTints()
        cleanupMcpShortcutsListener()
        cleanupMcpMainBridge()
        cleanupRestrictedSettingsBridge()
        cleanupAutoSendToastListener()
        cleanupQuitPrompt()
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
<!-- The one sign-in sheet. Every credential ask in the app opens THIS one
     (`$lib/servers/sign-in-sheet-state.svelte.ts`), so a second protocol never
     means a second dialog. Keyed on the request, so a sheet opened while one is
     already up starts from a clean form rather than inheriting the old one's. -->
{#if signInRequest}
    {#key signInRequest}
        <SignInSheet request={signInRequest} onDone={closeSignInSheet} />
    {/key}
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
{#if updateBlockerNotice.blocker}
    <MoveToApplicationsDialog blocker={updateBlockerNotice.blocker} onClose={dismissMoveToApplicationsNudge} />
{/if}
{#if import.meta.env.DEV || __CMDR_E2E_BUILD__}
    <DialogGallery />
{/if}
<div class="page-wrapper">
    {#if settingsReady}
        {@render children?.()}
    {/if}
</div>
<!-- Last in the markup AND `topmost` (`--z-modal-top`): the quit prompt has to
     be answerable over anything already open, a modal conflict dialog included.
     The z-index is what actually guarantees it; the position here just keeps the
     DOM honest about the intent. -->
{#if quitPrompt.open}
    <QuitConfirmationDialog
        operations={quitPrompt.operations}
        secondsLeft={quitPrompt.secondsLeft}
        onQuit={() => {
            quitPrompt.confirm()
        }}
        onKeepWorking={() => {
            quitPrompt.keepWorking()
        }}
    />
{/if}

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
