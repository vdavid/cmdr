<script lang="ts">
    /**
     * The last boundary in every window, around the root layout's content: whatever gets
     * past the dialog and pane boundaries (and SvelteKit's own per-route ones) lands here and
     * shows `PageCrashPanel` instead of a blank window.
     *
     * Reload first re-arms the boundary in place, which keeps the window and its backend
     * subscriptions. If the window breaks again within a few seconds of that, the next Reload
     * reloads the webview from scratch. Layers: `apps/desktop/src/DETAILS.md` § "Error
     * boundaries".
     */
    import type { Snippet } from 'svelte'
    import PageCrashPanel from './PageCrashPanel.svelte'
    import { reloadWindow } from './reload-window'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { describeForLog } from '#lib/logging/describe-for-log.js'

    const { children }: { children: Snippet } = $props()

    const log = getAppLogger('windowBoundary')

    /** A break this soon after a re-arm means re-arming doesn't help, so the next Reload starts the webview over. */
    const REARM_RETRY_WINDOW_MS = 5000
    let lastRearmAt = 0
    let rearmDidNotHelp = false

    function handleError(error: unknown): void {
        log.error('This window broke past every inner boundary; showing the crash panel: {error}', { error })
        rearmDidNotHelp = Date.now() - lastRearmAt < REARM_RETRY_WINDOW_MS
    }

    function reload(reset: () => void): void {
        if (rearmDidNotHelp) {
            reloadWindow()
            return
        }
        lastRearmAt = Date.now()
        reset()
    }

    /**
     * The throw's readable `Name: message`. Under SvelteKit a boundary receives what
     * `hooks.client.ts` made of the throw, which carries it as `detail`; anything else
     * (a raw throw, in tests) goes through the log bridge's renderer.
     */
    function detailOf(error: unknown): string {
        if (typeof error === 'object' && error !== null && 'detail' in error && typeof error.detail === 'string') {
            return error.detail
        }
        return describeForLog(error, { marks: false })
    }
</script>

<svelte:boundary onerror={handleError}>
    {@render children()}

    {#snippet failed(error: unknown, reset: () => void)}
        <PageCrashPanel
            detail={detailOf(error)}
            onReload={() => {
                reload(reset)
            }}
        />
    {/snippet}
</svelte:boundary>
