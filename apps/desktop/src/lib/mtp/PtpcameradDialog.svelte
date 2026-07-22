<script lang="ts">
    import { onMount } from 'svelte'
    import { getPtpcameradWorkaroundCommand } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import CommandBox from '$lib/ui/CommandBox.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import type { Snippet } from 'svelte'

    interface Props {
        /** The process name that's blocking (like "pid 45145, ptpcamerad"). */
        blockingProcess?: string
        /** Called when the dialog is closed. */
        onClose: () => void
        /** Called when user wants to retry connecting. */
        onRetry: () => void
    }

    const { blockingProcess, onClose, onRetry }: Props = $props()

    let workaroundCommand = $state('')

    onMount(async () => {
        workaroundCommand = await getPtpcameradWorkaroundCommand()
    })

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onRetry()
        }
    }
</script>

{#snippet processName(children: Snippet)}<strong>{@render children()}</strong>{/snippet}
{#snippet codeTag(children: Snippet)}<code>{@render children()}</code>{/snippet}
{#snippet ctrlCChip(children: Snippet)}<ShortcutChip key="Ctrl+C" />{@render children()}{/snippet}

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    blur
    dialogId="ptpcamerad"
    onclose={onClose}
    containerStyle="min-width: 480px; max-width: 560px"
>
    {#snippet title()}{tString('mtp.ptpcameradDialog.title')}{/snippet}

    <div class="dialog-body">
        <p class="description">
            {#if blockingProcess}
                <Trans
                    key="mtp.ptpcameradDialog.inUseBy"
                    snippets={{ process: processName }}
                    params={{ process: blockingProcess }}
                />
            {:else}
                {tString('mtp.ptpcameradDialog.inUseGeneric')}
            {/if}
        </p>

        <p class="explanation">
            <Trans key="mtp.ptpcameradDialog.explanation" snippets={{ code: codeTag }} />
        </p>

        <div class="command-wrapper">
            {#if workaroundCommand}
                <CommandBox command={workaroundCommand} />
            {/if}
        </div>

        <p class="help-text">
            <Trans key="mtp.ptpcameradDialog.helpText" snippets={{ key: ctrlCChip }} />
        </p>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onClose}>{tString('mtp.ptpcameradDialog.close')}</Button>
        <Button variant="primary" onclick={onRetry}>{tString('mtp.ptpcameradDialog.retry')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .description strong {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .explanation {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.6;
    }

    .explanation code {
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .command-wrapper {
        margin-bottom: var(--spacing-md);
    }

    .help-text {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }
</style>
