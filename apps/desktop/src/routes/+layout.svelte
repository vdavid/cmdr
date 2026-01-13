<script lang="ts">
    import { onMount } from 'svelte'
    import '../app.css'
    import { initWindowStateListener } from '$lib/window-state'
    import { startUpdateChecker } from '$lib/updater.svelte'
    import UpdateNotification from '$lib/UpdateNotification.svelte'

    onMount(() => {
        // Initialize window state persistence on resize
        // This ensures window size/position survives hot reloads
        void initWindowStateListener()

        // Start checking for updates (skips in dev mode)
        return startUpdateChecker()
    })
</script>

<UpdateNotification />
<slot />
