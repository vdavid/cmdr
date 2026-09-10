<script lang="ts">
    /**
     * The once-ever INFO toast that fires when the switcher's Network group first
     * holds five pinned servers: how to shorten it, and the promise that
     * unpinning loses nothing.
     *
     * `$lib/stores/volume-store` raises it and sets
     * `behavior.serversPinHintSeen`, so this component only has to say "Got it".
     *
     * ❗ The palette entry is named through its OWN catalog key, ❌ never a second
     * copy of the words: a translated toast pointing at an English command name
     * sends the reader looking for something they can't find.
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { getMessage, tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** Dedup id of this toast, so "Got it" can retire it. */
        toastId: string
        /** Whether the user's favorites are piling up too, which earns a line. */
        mentionFavorites: boolean
    }

    const { toastId, mentionFavorites }: Props = $props()

    const body = $derived(
        tString('servers.pinHint.body', { command: getMessage('commands.serversTogglePin.label') }),
    )
</script>

<div class="content">
    <strong class="title">{tString('servers.pinHint.title')}</strong>
    <span class="body">{body}</span>
    {#if mentionFavorites}
        <span class="body">{tString('servers.pinHint.favorites')}</span>
    {/if}
    <div class="actions">
        <Button
            size="mini"
            variant="primary"
            onclick={() => {
                dismissToast(toastId)
            }}>{tString('servers.pinHint.gotIt')}</Button
        >
    </div>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-md);
    }
</style>
