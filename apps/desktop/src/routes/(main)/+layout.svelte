<script lang="ts">
    /**
     * Main window layout - includes updater, notifications, window state, and settings.
     * Only used for the main file manager window.
     */
    import { onMount, onDestroy } from 'svelte'
    import { initWindowStateListener } from '$lib/window-state'
    import { startUpdateChecker } from '$lib/updater.svelte'
    import { initSettingsApplier, cleanupSettingsApplier } from '$lib/settings/settings-applier'
    import { initReactiveSettings, cleanupReactiveSettings } from '$lib/settings/reactive-settings.svelte'
    import AiNotification from '$lib/AiNotification.svelte'
    import UpdateNotification from '$lib/UpdateNotification.svelte'

    let cleanupUpdater: (() => void) | undefined

    onMount(async () => {
        // Initialize reactive settings for UI components
        await initReactiveSettings()

        // Initialize settings and apply them to CSS variables
        await initSettingsApplier()

        // Initialize window state persistence on resize
        // This ensures window size/position survives hot reloads
        void initWindowStateListener()

        // Start checking for updates (skips in dev mode)
        cleanupUpdater = startUpdateChecker()
    })

    onDestroy(() => {
        cleanupReactiveSettings()
        cleanupSettingsApplier()
        cleanupUpdater?.()
    })
</script>

<UpdateNotification />
<AiNotification />
<div class="page-wrapper">
    <slot />
</div>

<style>
    .page-wrapper {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
    }
</style>
