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
                    Copy this selection to the clipboard?
                {:else}
                    Copy {formatBytes(bytes)} to the clipboard?
                {/if}
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">Large pastes can slow down other apps. Try search (⌘F) to narrow it down.</p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={onCancelConfirm}>Cancel</Button>
                <Button variant="secondary" onclick={onSaveAs}>Save as file…</Button>
                <Button variant="primary" autoFocus onclick={onProceedConfirm}>Copy</Button>
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
                Copy {formatBytes(bytes)} to the clipboard?
            </h2>
        {/snippet}
        <div class="copy-dialog-body-wrap">
            <p class="copy-dialog-body">
                That's larger than the 100 MB clipboard limit. Try search (⌘F) to find what you need, or save the
                selection as a file.
            </p>
            <div class="copy-dialog-actions">
                <Button variant="secondary" onclick={onDismissRefuse}>Cancel</Button>
                <Button variant="primary" autoFocus onclick={onSaveAs}>Save as file…</Button>
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
