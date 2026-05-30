<script lang="ts">
    /**
     * Root layout - minimal, just CSS and logger.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount } from 'svelte'
    import '../app.css'
    import { initLogger } from '$lib/logging/logger'
    import { installClipboardShimIfE2e } from '$lib/clipboard-shim'

    onMount(() => {
        void initLogger()
        // E2E-only: keep webview clipboard writes off the real OS clipboard.
        // No-op in dev/prod. Runs for every window (main, viewer, debug).
        void installClipboardShimIfE2e()
    })
</script>

<slot />
