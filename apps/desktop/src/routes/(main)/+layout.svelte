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
    import { onMtpExclusiveAccessError, connectMtpDevice, type MtpExclusiveAccessErrorEvent } from '$lib/tauri-commands'
    import { initAiState } from '$lib/ai/ai-state.svelte'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import { PtpcameradDialog } from '$lib/mtp'
    import type { Snippet } from 'svelte'

    interface Props {
        children?: Snippet
    }

    const { children }: Props = $props()

    // State for ptpcamerad dialog
    let showPtpcameradDialog = $state(false)
    let ptpcameradBlockingProcess = $state<string | undefined>(undefined)
    let pendingDeviceId = $state<string | undefined>(undefined)

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

    // Cleanup functions stored for onDestroy
    let mtpUnlistenPromise: Promise<() => void> | undefined
    let updateCleanup: (() => void) | undefined
    let aiCleanup: (() => void) | undefined

    onMount(() => {
        // Initialize all async setup
        void (async () => {
            // Initialize reactive settings for UI components
            await initReactiveSettings()

            // Initialize settings and apply them to CSS variables
            await initSettingsApplier()

            // Read system accent color from macOS and listen for changes
            await initAccentColor()

            // Initialize keyboard shortcuts store (loads custom shortcuts from disk)
            await initializeShortcuts()

            // Set up MCP shortcuts listener (allows MCP tools to modify shortcuts)
            await setupMcpShortcutsListener()

            // Initialize window state persistence on resize
            // This ensures window size/position survives hot reloads
            void initWindowStateListener()

            // Listen for MTP exclusive access errors
            mtpUnlistenPromise = onMtpExclusiveAccessError(handleMtpExclusiveAccessError)

            // Start checking for updates (skips in dev mode)
            updateCleanup = startUpdateChecker()

            // Initialize AI state and event listeners (shows offer toast if eligible)
            aiCleanup = await initAiState()
        })()
    })

    onDestroy(() => {
        // Cleanup MTP listener
        void mtpUnlistenPromise?.then((unlisten) => {
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
    })
</script>

<ToastContainer />
{#if showPtpcameradDialog}
    <PtpcameradDialog
        blockingProcess={ptpcameradBlockingProcess}
        onClose={closePtpcameradDialog}
        onRetry={retryMtpConnection}
    />
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
