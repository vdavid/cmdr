<script lang="ts">
    /**
     * Root layout - CSS, logger, and the settings every window needs.
     * Main-window-specific features (updater, notifications) are in (main)/+layout.svelte.
     * Other windows (viewer, debug) get only this minimal layout.
     */
    import { onMount, type Snippet } from 'svelte'
    // Global stylesheets, in cascade order. The five after `app.css` must stay last;
    // importing them here is what keeps them there. Don't move them into an `@import`
    // inside `app.css`: that hoists to the top of the sheet and inverts the cascade.
    // See `apps/desktop/src/DETAILS.md` § Global stylesheets.
    import '../app.css'
    import '../app-field.css'
    import '../app-utilities.css'
    import '../app-tooltip.css'
    import '../app-file-list.css'
    import { initLogger } from '#lib/logging/logger.js'
    import { installClipboardShimIfE2e } from '#lib/clipboard-shim.js'
    import { initWindowSettings } from '#lib/settings/window-settings.js'
    import { initAppMode } from '#lib/app-mode.js'
    import { installContextMenuOwnership } from '#lib/context-menu-ownership.js'
    import WindowBoundary from '#lib/error-reporter/WindowBoundary.svelte'

    const { children }: { children?: Snippet } = $props()

    onMount(() => {
        // No WebKit page menu ("Reload", "Inspect Element") in ANY window: text fields
        // keep their editing menu, everything else gets Cmdr's own menus or none.
        // Here, not per window, so a new window route can't forget it.
        const uninstallContextMenuOwnership = installContextMenuOwnership()
        void initLogger()
        // The run mode (E2E, screenshot capture) for EVERY window, not only the ones that
        // await it themselves: each window is its own JS context, so a Settings or queue
        // window that skipped this would answer `isE2eRun()` false for the whole run.
        // Promise-memoized, so a page awaiting it in its own `onMount` shares this call.
        void initAppMode()
        // E2E-only: keep webview clipboard writes off the real OS clipboard.
        // No-op in dev/prod. Runs for every window (main, viewer, debug).
        void installClipboardShimIfE2e()
        // Settings + the reactive layer, for EVERY window. Living here is what
        // makes it impossible for a new window route to forget and render every
        // reactive setting at its registry default (sizes in binary when the
        // user picked SI, and so on). Promise-memoized, so a page that awaits
        // the same call in its own `onMount` shares this run.
        void initWindowSettings()
        return uninstallContextMenuOwnership
    })
</script>

<!-- Every window renders through this layout, so this one boundary is what makes a blank
     window impossible in all of them. `apps/desktop/src/DETAILS.md` § "Error boundaries". -->
<WindowBoundary>
    {@render children?.()}
</WindowBoundary>
