<script lang="ts">
    import { onMount } from 'svelte'
    import '../app.css'
    import { initWindowStateListener } from '$lib/window-state'
    import { startUpdateChecker } from '$lib/updater.svelte'
    import AiNotification from '$lib/AiNotification.svelte'
    import UpdateNotification from '$lib/UpdateNotification.svelte'
    import { initLogger } from '$lib/logger'

    onMount(() => {
        // Initialize logging first
        void initLogger()
        // Initialize window state persistence on resize
        // This ensures window size/position survives hot reloads
        void initWindowStateListener()

        // Start checking for updates (skips in dev mode)
        return startUpdateChecker()
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
