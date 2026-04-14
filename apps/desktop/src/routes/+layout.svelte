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

    // SvelteKit's client router crashes with "Cannot access 'component' before
    // initialization" when HMR updates hit the root layout (virtual:uno.css, app.css).
    // Catch the crash and force a clean page reload.
    if (import.meta.hot) {
        window.addEventListener('unhandledrejection', (event) => {
            if (
                event.reason instanceof ReferenceError &&
                event.reason.message.includes('component')
            ) {
                event.preventDefault()
                location.reload()
            }
        })
    }

    onMount(() => {
        void initLogger()
    })
</script>

<slot />
