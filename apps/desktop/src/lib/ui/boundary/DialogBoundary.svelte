<script lang="ts">
    /**
     * Walls one dialog (or a group of them, or the Ask Cmdr rail) off from the rest
     * of the window.
     *
     * A dialog that throws while it renders or tears down closes, a toast says so, the
     * throw is reported, and focus goes home, while the window stays exactly as it was.
     * Without it the throw reaches the window's boundary and replaces the whole window.
     *
     * ❗ Wrap the `{#if}` that mounts the dialog, never sit inside it. Svelte routes a
     * teardown throw to the nearest LIVE ancestor boundary and skips one that is itself
     * tearing down, so a boundary inside the `{#if}` dies with the dialog and the throw
     * goes over its head. `cmdr/dialog-inside-boundary` holds every route to this.
     * Layers and rules: `apps/desktop/src/DETAILS.md` § "Error boundaries".
     */
    import type { Snippet } from 'svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { addToast } from '#lib/ui/toast/index.js'
    import { focusHome } from '#lib/ui/focus-restore.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        /** Names the dialog in the log line, so a report says which one broke. */
        name: string
        /**
         * Closes the dialog: clears whatever state mounts it, so the re-armed boundary
         * renders nothing. Usually the dialog's own close handler. Omit it only for a
         * dialog that owns its visibility, which the remount itself starts closed.
         */
        onClose?: () => void
        /**
         * The toast's text, already translated. Defaults to the generic "That dialog ran
         * into a problem" line; a surface the user knows by name (the Ask Cmdr rail)
         * names itself.
         */
        closedMessage?: string
        children: Snippet
    }

    const { name, onClose, closedMessage, children }: Props = $props()

    const log = getAppLogger('dialogBoundary')

    /**
     * How many failures in a row may re-arm the boundary before it stays down. Closing
     * the dialog makes the retry render nothing, so it can't throw again; the cap is for
     * a host whose close doesn't really close, so a throw-on-every-render can't spin.
     */
    const MAX_CONSECUTIVE_RESETS = 3
    /** Failures further apart than this start a fresh count, so one bad moment doesn't
     *  cost the rest of the session this dialog. A re-render loop lands well inside it. */
    const FAILURE_WINDOW_MS = 5000
    let consecutiveFailures = 0
    let lastFailureAt = 0

    /**
     * Closes the broken dialog, tells the user, reports, then re-arms.
     *
     * The re-arm is deferred to the next tick, `setTimeout(0)` (❌ never
     * `requestAnimationFrame`, which macOS throttles in unfocused windows), so the close
     * has flushed before the children render again. Without a reset the boundary stays
     * failed and this dialog never opens again for the rest of the session.
     */
    function handleError(error: unknown, reset: () => void): void {
        log.error('The {name} dialog threw; closing it and handing focus back: {error}', { name, error })
        addToast(closedMessage ?? tString('ui.dialogBoundary.closedToast'), { level: 'error' })
        try {
            onClose?.()
        } catch (closeError) {
            log.error("The {name} dialog's close threw too: {error}", { name, error: closeError })
        }
        const now = Date.now()
        consecutiveFailures = now - lastFailureAt > FAILURE_WINDOW_MS ? 1 : consecutiveFailures + 1
        lastFailureAt = now
        setTimeout(() => {
            focusHome()
            if (consecutiveFailures <= MAX_CONSECUTIVE_RESETS) reset()
        }, 0)
    }
</script>

<svelte:boundary onerror={handleError}>
    {@render children()}

    {#snippet failed()}
        <!-- Deliberately empty: the dialog is closed and focus is on its way home. -->
    {/snippet}
</svelte:boundary>
