<script lang="ts">
    /**
     * Root layout - minimal, just CSS and logger.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount } from 'svelte'
    // Global stylesheets, in cascade order. The three after `app.css` must stay last;
    // importing them here is what keeps them there. Don't move them into an `@import`
    // inside `app.css`: that hoists to the top of the sheet and inverts the cascade.
    // See `apps/desktop/src/DETAILS.md` § Global stylesheets.
    import '../app.css'
    import '../app-field.css'
    import '../app-utilities.css'
    import '../app-tooltip.css'
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
