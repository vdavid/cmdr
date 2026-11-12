<!--
    Copy confirmation (10 to 100 MiB band) and refusal (> 100 MiB) modals for the
    viewer. Presentational: the page owns all the copy-flow state
    (`copyConfirmBytes`, `copyRefuseBytes`, the pending `proceed` thunk) and the
    IPC-bound handlers; this component renders the open dialog and reports the
    user's choice back through callback props.
-->

<script lang="ts">
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatByteSize } from '#lib/units/index.js'
    import type { Snippet } from 'svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'

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

<!-- The search key both bodies point at, live in this OS's spelling. `viewer.search` is a fixed key,
     so it's always bound: the sentence never loses its key. Non-clickable: a chip's deep link opens
     Settings, which this restricted window can't create. -->
{#snippet searchChip(_children: Snippet)}<ShortcutChip commandId="viewer.search" clickable={false} />{/snippet}

{#if confirmBytes !== null}
    {@const bytes = confirmBytes}
    <ModalDialog
        dialogId="viewer-copy-confirm"
        titleId="viewer-copy-confirm-title"
        onclose={onCancelConfirm}
        containerStyle="max-width: 480px"
    >
        {#snippet title()}
            {#if bytes === -1}
                {tString('viewer.copyDialog.confirmTitleUnknown')}
            {:else}
                {tString('viewer.copyDialog.confirmTitleKnown', { size: formatByteSize(bytes) })}
            {/if}
        {/snippet}
        <p class="copy-dialog-body">
            <Trans key="viewer.copyDialog.confirmMessage" snippets={{ key: searchChip }} />
        </p>
        {#snippet footer()}
            <Button variant="secondary" onclick={onCancelConfirm}>{tString('viewer.copyDialog.cancel')}</Button>
            <Button variant="secondary" onclick={onSaveAs}>{tString('viewer.copyDialog.saveAsFile')}</Button>
            <Button variant="primary" autoFocus onclick={onProceedConfirm}>{tString('viewer.copyDialog.copy')}</Button>
        {/snippet}
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
            {tString('viewer.copyDialog.confirmTitleKnown', { size: formatByteSize(bytes) })}
        {/snippet}
        <p class="copy-dialog-body">
            <Trans key="viewer.copyDialog.refuseMessage" snippets={{ key: searchChip }} />
        </p>
        {#snippet footer()}
            <Button variant="secondary" onclick={onDismissRefuse}>{tString('viewer.copyDialog.cancel')}</Button>
            <Button variant="primary" autoFocus onclick={onSaveAs}>{tString('viewer.copyDialog.saveAsFile')}</Button>
        {/snippet}
    </ModalDialog>
{/if}

<style>
    .copy-dialog-body {
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-normal);
        color: var(--color-text-secondary);
        margin: 0;
    }
</style>
