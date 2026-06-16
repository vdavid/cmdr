<!--
    Copy confirmation (10 to 100 MiB band) and refusal (> 100 MiB) modals for the
    viewer. Presentational: the page owns all the copy-flow state
    (`copyConfirmBytes`, `copyRefuseBytes`, the pending `proceed` thunk) and the
    IPC-bound handlers; this component renders the open dialog and reports the
    user's choice back through callback props.
-->

<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { formatBytes } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /**
         * Byte count for the confirm dialog when open, or `null` when closed.
         * `-1` is the "unknown size" sentinel (ByteSeek range we never scrolled
         * through) and renders a size-free prompt.
         */
        confirmBytes: number | null
        /** Byte count for the refuse dialog when open, or `null` when closed. */
        refuseBytes: number | null
        /** Cancel / dismiss the confirm dialog. */
        onCancelConfirm: () => void
        /** Proceed with the copy from the confirm dialog. */
        onProceedConfirm: () => void
        /** Dismiss the refuse dialog. */
        onDismissRefuse: () => void
        /** Open the native save panel and stream the selection to a file. */
        onSaveAs: () => void
    }

    const { confirmBytes, refuseBytes, onCancelConfirm, onProceedConfirm, onDismissRefuse, onSaveAs }: Props = $props()
</script>

{#if confirmBytes !== null}
    {@const bytes = confirmBytes}
    <ModalDialog
        dialogId="viewer-copy-confirm"
        titleId="viewer-copy-confirm-title"
        onclose={onCancelConfirm}
        containerStyle="max-width: 480px"
    >
        {#snippet title()}
            <h2 id="viewer-copy-confirm-title" class="copy-dialog-title">
                {#if bytes === -1}
                    {tString('viewer.copyDialog.confirmTitleUnknown')}
                {:else}
                    {tString('viewer.copyDialog.confirmTitleKnown', { size: formatBytes(bytes) })}
                {/if}
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">{tString('viewer.copyDialog.confirmBody')}</p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={onCancelConfirm}>{tString('viewer.copyDialog.cancel')}</Button>
                <Button variant="secondary" onclick={onSaveAs}>{tString('viewer.copyDialog.saveAsFile')}</Button>
                <Button variant="primary" autoFocus onclick={onProceedConfirm}>{tString('viewer.copyDialog.copy')}</Button>
            </div>
        </div>
    </ModalDialog>
{/if}

{#if refuseBytes !== null}
    {@const bytes = refuseBytes}
    <ModalDialog
        dialogId="viewer-copy-refuse"
        titleId="viewer-copy-refuse-title"
        onclose={onDismissRefuse}
        containerStyle="max-width: 480px"
    >
        {#snippet title()}
            <h2 id="viewer-copy-refuse-title" class="copy-dialog-title">
                {tString('viewer.copyDialog.confirmTitleKnown', { size: formatBytes(bytes) })}
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">
                {tString('viewer.copyDialog.refuseBody')}
            </p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={onDismissRefuse}>{tString('viewer.copyDialog.cancel')}</Button>
                <Button variant="primary" autoFocus onclick={onSaveAs}>{tString('viewer.copyDialog.saveAsFile')}</Button>
            </div>
        </div>
    </ModalDialog>
{/if}

<style>
    .copy-dialog-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        text-align: center;
        margin: 0;
    }

    /* Matches the AlertDialog body wrapper: design-system § Dialogs body padding 0 24px 24px. */
    .copy-dialog-body-wrap {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .copy-dialog-body {
        font-size: var(--font-size-md);
        line-height: 1.4;
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-xl);
    }

    .copy-dialog-actions {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }
</style>
