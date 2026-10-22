<script lang="ts">
    /**
     * Walls one file pane off from the other. A pane that throws while it renders or
     * tears down shows a calm "This pane hit a problem" with a Reload, and the other pane,
     * the tab bar, and every dialog keep working.
     *
     * It wraps the `{#key}` that remounts the pane per tab, ❌ never sits inside it: Svelte
     * routes a teardown throw to the nearest LIVE ancestor boundary, and one inside the
     * `{#key}` would be tearing down with the pane it guards. Layers:
     * `apps/desktop/src/DETAILS.md` § "Error boundaries".
     */
    import { untrack, type Snippet } from 'svelte'
    import Button from '#lib/ui/Button.svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { focusHome } from '#lib/ui/focus-restore.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { dependOn } from '#lib/utils/reactivity.js'

    interface Props {
        paneId: 'left' | 'right'
        /**
         * The pane's active tab. A change re-arms a failed boundary, so switching tabs
         * shows the other tab instead of this tab's problem.
         */
        resetKey: unknown
        children: Snippet
    }

    const { paneId, resetKey, children }: Props = $props()

    const log = getAppLogger('paneBoundary')

    /** The failed boundary's `reset`, held so a tab switch can re-arm it. `null` while healthy. */
    let pendingReset: (() => void) | null = null

    function handleError(error: unknown, reset: () => void): void {
        log.error('The {paneId} pane threw; showing its reload panel: {error}', { paneId, error })
        pendingReset = reset
    }

    /** Remounts the pane and, on the next tick, puts focus back where keys reach a pane. */
    function reload(): void {
        const reset = pendingReset
        pendingReset = null
        reset?.()
        setTimeout(() => {
            focusHome()
        }, 0)
    }

    $effect(() => {
        dependOn(resetKey)
        untrack(() => {
            if (pendingReset) reload()
        })
    })
</script>

<svelte:boundary onerror={handleError}>
    {@render children()}

    {#snippet failed()}
        <div class="pane-problem" role="alert">
            <h2 class="title">{tString('fileExplorer.paneBoundary.title')}</h2>
            <p class="body">{tString('fileExplorer.paneBoundary.body')}</p>
            <Button variant="primary" onclick={reload}>{tString('fileExplorer.paneBoundary.reload')}</Button>
        </div>
    {/snippet}
</svelte:boundary>

<style>
    .pane-problem {
        display: flex;
        flex: 1;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        min-height: 0;
        padding: var(--spacing-xl);
        line-height: var(--font-line-height-prose);
        text-align: center;
        background-color: var(--color-bg-primary);
    }

    .title {
        margin: 0;
        font-size: var(--font-size-xl);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .body {
        max-width: 450px;
        margin: 0 0 var(--spacing-sm) 0;
        color: var(--color-text-secondary);
    }
</style>
