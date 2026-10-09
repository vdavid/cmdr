<script lang="ts">
    /**
     * The INFO toast a favorite pick raises when the pane can't go there (an unplugged
     * phone or drive, phones switched off, a forgotten server, a folder that's gone), with
     * the one way out when there is one. `navigation/open-favorite.ts` raises it and
     * decides both the words and the action (`favorite-reach.ts`); this only renders them.
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'

    interface Props {
        /** Dedup id of this toast, so the action can retire it. */
        toastId: string
        message: string
        /** The button's label, or `null` for a toast that only informs. */
        actionLabel: string | null
        onAction: () => void
    }

    const { toastId, message, actionLabel, onAction }: Props = $props()
</script>

<div class="content">
    <span class="body">{message}</span>
    {#if actionLabel}
        <div class="actions">
            <Button
                size="mini"
                variant="secondary"
                onclick={() => {
                    dismissToast(toastId)
                    onAction()
                }}>{actionLabel}</Button
            >
        </div>
    {/if}
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .body {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
