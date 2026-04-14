<script lang="ts">
    /**
     * Root layout - minimal, just CSS and logger.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount } from 'svelte'
    import 'virtual:uno.css'
    import '../app.css'
    import { initLogger } from '$lib/logging/logger'

    // When the root layout or its dependencies (virtual:uno.css, app.css) change,
    // SvelteKit's client router crashes with "Cannot access 'component' before
    // initialization." Force a clean page reload instead of the broken HMR path.
    if (import.meta.hot) {
        const hot = import.meta.hot
        hot.accept(() => {
            hot.invalidate()
        })
    }

    onMount(() => {
        void initLogger()
    })
</script>

<slot />
