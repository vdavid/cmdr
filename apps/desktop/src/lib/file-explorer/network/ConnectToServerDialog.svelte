<script lang="ts">
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { connectToServer } from '$lib/tauri-commands'
    import { triggerNetworkDiscovery } from './lazy-trigger'
    import { tString } from '$lib/intl/messages.svelte'
    import type { NetworkHost } from '../types'

    type DialogState = 'idle' | 'connecting' | 'error'

    interface Props {
        onConnect: (host: NetworkHost, sharePath: string | null) => void
        onClose: () => void
    }

    const { onConnect, onClose }: Props = $props()

    let address = $state('')
    let dialogState = $state<DialogState>('idle')
    let errorMessage = $state('')
    let inputRef: HTMLInputElement | undefined = $state()

    const canSubmit = $derived(address.trim().length > 0 && dialogState !== 'connecting')

    onMount(async () => {
        // Lazy-start mDNS: opening this dialog signals intent to do networking, and
        // `connectToServer` itself opens a TCP socket to a private IP (which would also
        // trigger the macOS Local Network prompt on its own). Triggering here first means
        // the prompt fires alongside the dialog rather than after the user hits Connect.
        triggerNetworkDiscovery()

        await tick()
        inputRef?.focus()
    })

    async function handleSubmit() {
        const trimmed = address.trim()
        if (!trimmed || dialogState === 'connecting') return

        dialogState = 'connecting'
        errorMessage = ''

        try {
            const result = await connectToServer(trimmed)
            onConnect(result.host, result.sharePath)
        } catch (e) {
            errorMessage = String(e)
            dialogState = 'error'
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && canSubmit) {
            void handleSubmit()
        }
    }
</script>

<ModalDialog
    titleId="connect-to-server-title"
    dialogId="connect-to-server"
    onclose={onClose}
    onkeydown={handleKeydown}
    containerStyle="width: 400px"
>
    {#snippet title()}{tString('fileExplorer.network.connectDialog.title')}{/snippet}

    <div class="dialog-body">
        <div class="input-group">
            <TextInput
                bind:inputElement={inputRef}
                bind:value={address}
                invalid={dialogState === 'error'}
                ariaLabel={tString('fileExplorer.network.connectDialog.addressAriaLabel')}
                aria-describedby={errorMessage ? 'connect-error' : 'connect-help'}
                spellcheck={false}
                autocomplete="off"
                placeholder={tString('fileExplorer.network.connectDialog.addressPlaceholder')}
                disabled={dialogState === 'connecting'}
            />
            <p id="connect-help" class="help-text">{tString('fileExplorer.network.connectDialog.examples')}</p>
            {#if errorMessage}
                <p id="connect-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onClose}>{tString('fileExplorer.network.cancel')}</Button>
        <Button variant="primary" onclick={() => void handleSubmit()} disabled={!canSubmit}>
            {#if dialogState === 'connecting'}
                <Spinner size="sm" />
                {tString('fileExplorer.network.connecting')}
            {:else}
                {tString('fileExplorer.network.connect')}
            {/if}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .help-text {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .error-message {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-error);
    }
</style>
