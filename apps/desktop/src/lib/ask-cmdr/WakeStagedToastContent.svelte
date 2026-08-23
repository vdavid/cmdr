<script lang="ts">
    /**
     * The notice that Ask Cmdr noticed something on its own and staged it for review.
     *
     * A component rather than a string, because the whole point is the way in: a toast saying
     * something is waiting, with no way to get to it, just asks the user to go looking.
     *
     * Two destinations, and they answer different questions. "Review" opens the suggestions,
     * which is WHAT it wants to do. The thread link opens the chat it reasoned in, which is
     * WHY — and that half matters, because nobody asked for any of this.
     */
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getAppLogger } from '$lib/logging/logger'
    import { openSuggestedOps } from '$lib/suggested-ops/suggested-ops-trigger.svelte'
    import { openRail, switchToThread } from './ask-cmdr-trigger.svelte'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
        /** The thread the wake reasoned in, so the user can read why it proposed this. */
        conversationId: number
        /** How many proposals it staged. Always at least one. */
        proposals: number
    }
    const { toastId, conversationId, proposals }: Props = $props()

    const log = getAppLogger('askCmdr')

    const title = $derived(
        tString('askCmdr.wakeToast.title', { countText: formatInteger(proposals), count: proposals }),
    )

    function review(): void {
        void openSuggestedOps()
        // The review surface now owns this conversation, in full. Leaving the toast behind it
        // would be two voices saying the same thing.
        dismissToast(toastId)
    }

    async function openThread(): Promise<void> {
        // ⚠️ `switchToThread` BEFORE `openRail`: a closed→open transition otherwise bootstraps
        // the most recent thread and wastes a fetch on one we are about to replace.
        await switchToThread(conversationId)
        await openRail()
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span class="title">
        <span class="glyph" aria-hidden="true"><Icon name="bot" size={15} /></span>
        {title}
    </span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={review}>{tString('askCmdr.wakeToast.action')}</Button>
        <button
            type="button"
            class="thread-link"
            onclick={() =>
                void openThread().catch((error: unknown) => {
                    log.warn("Couldn't open the wake's thread from its toast: {error}", { error: String(error) })
                })}
        >
            {tString('askCmdr.wakeToast.openThread')}
        </button>
    </div>
</div>

<style>
    .toast-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .title {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .glyph {
        display: flex;
        flex: none;
        color: var(--color-text-secondary);
    }

    .actions {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* A link-shaped second action, so it reads as the quieter of the two. `--color-accent-text`
       rather than `--color-accent`, which has too little contrast as foreground. */
    .thread-link {
        padding: 0;
        font: inherit;
        font-size: var(--font-size-xs);
        color: var(--color-accent-text);
        background: none;
        border: none;
        text-decoration: underline;
    }

    .thread-link:hover {
        text-decoration-thickness: 2px;
    }
</style>
