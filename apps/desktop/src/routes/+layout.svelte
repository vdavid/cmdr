<script lang="ts">
    /**
     * Root layout - minimal, just CSS and logger.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount } from 'svelte'
    import '../app.css'
    import { initLogger } from '$lib/logger'

    onMount(() => {
        void initLogger()
    })
</script>

<slot />
