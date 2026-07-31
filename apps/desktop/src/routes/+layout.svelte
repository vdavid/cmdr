<script lang="ts">
    /**
     * Root layout - CSS, logger, and the settings every window needs.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount } from 'svelte'
    // Global stylesheets, in cascade order. The four after `app.css` must stay last;
    // importing them here is what keeps them there. Don't move them into an `@import`
    // inside `app.css`: that hoists to the top of the sheet and inverts the cascade.
    // See `apps/desktop/src/DETAILS.md` § Global stylesheets.
    import '../app.css'
    import '../app-field.css'
    import '../app-utilities.css'
    import '../app-tooltip.css'
    import '../app-file-list.css'
    import { initLogger } from '$lib/logging/logger'
    import { installClipboardShimIfE2e } from '$lib/clipboard-shim'
    import { initWindowSettings } from '$lib/settings/window-settings'

    onMount(() => {
        void initLogger()
        // E2E-only: keep webview clipboard writes off the real OS clipboard.
        // No-op in dev/prod. Runs for every window (main, viewer, debug).
        void installClipboardShimIfE2e()
        // Settings + the reactive layer, for EVERY window. Living here is what
        // makes it impossible for a new window route to forget and render every
        // reactive setting at its registry default (sizes in binary when the
        // user picked SI, and so on). Promise-memoized, so a page that awaits
        // the same call in its own `onMount` shares this run.
        void initWindowSettings()
    })
</script>

<slot />
