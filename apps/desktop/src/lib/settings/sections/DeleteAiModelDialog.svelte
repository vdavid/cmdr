<script lang="ts">
    /**
     * "Delete the local AI model?" confirmation, opened from `AiLocalSection`.
     *
     * Mid-delete the whole dialog changes: the title, a spinner body instead of
     * the size warning, and both buttons disabled (Escape and Enter included) so
     * an uninstall in flight can't be cancelled or double-fired.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The installed model's size, already formatted. Falls back when the status hasn't loaded. */
        modelSizeFormatted: string | null
        /** True while the uninstall is running. */
        isDeleting: boolean
        onConfirm: () => void
        onCancel: () => void
    }

    const { modelSizeFormatted, isDeleting, onConfirm, onCancel }: Props = $props()
</script>

<ModalDialog
    titleId="delete-ai-model-title"
    dialogId="delete-ai-model"
    role="alertdialog"
    onclose={() => {
        if (!isDeleting) onCancel()
    }}
    containerStyle="width: 400px"
    onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Enter' && !isDeleting) {
            onConfirm()
        }
    }}
>
    {#snippet title()}{isDeleting
            ? tString('ai.local.deleteDialogTitleDeleting')
            : tString('ai.local.deleteDialogTitle')}{/snippet}
    <div class="confirm-body">
        {#if isDeleting}
            <div class="deleting-status">
                <Spinner size="sm" />
                <span>{tString('ai.local.deletingStatus')}</span>
            </div>
        {:else}
            <p class="confirm-message">
                {t('ai.local.deleteConfirmMessage', { modelSize: modelSizeFormatted ?? '2.0 GB' })}
            </p>
        {/if}
    </div>
    {#snippet footer()}
        <Button variant="secondary" disabled={isDeleting} onclick={onCancel}>{tString('ai.local.cancel')}</Button>
        <Button variant="danger" disabled={isDeleting} onclick={onConfirm}>
            {isDeleting ? tString('ai.local.deleteButtonDeleting') : tString('ai.local.deleteButton')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .confirm-body {
        padding: 0 var(--spacing-xl);
    }

    .confirm-message {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .deleting-status {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
