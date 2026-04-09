<script lang="ts">
    import { onMount, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { connectToServer } from '$lib/tauri-commands'
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
    {#snippet title()}Connect to server{/snippet}

    <div class="dialog-body">
        <div class="input-group">
            <input
                bind:this={inputRef}
                bind:value={address}
                type="text"
                class="address-input"
                class:has-error={dialogState === 'error'}
                aria-label="Server address"
                aria-describedby={errorMessage ? 'connect-error' : 'connect-help'}
                aria-invalid={dialogState === 'error'}
                spellcheck="false"
                autocomplete="off"
                placeholder="hostname, IP address, or smb:// URL"
                disabled={dialogState === 'connecting'}
            />
            <p id="connect-help" class="help-text">Examples: mynas.local, 192.168.1.100, smb://server/share</p>
            {#if errorMessage}
                <p id="connect-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

        <div class="button-row">
            <Button variant="secondary" onclick={onClose}>Cancel</Button>
            <Button variant="primary" onclick={() => void handleSubmit()} disabled={!canSubmit}>
                {#if dialogState === 'connecting'}
                    <span class="spinner spinner-sm"></span>
                    Connecting...
                {:else}
                    Connect
                {/if}
            </Button>
        </div>
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .address-input {
        width: 100%;
        padding: var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .address-input.has-error {
        border-color: var(--color-error);
    }

    .address-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .address-input:focus {
        outline: none;
        box-shadow: var(--shadow-focus);
    }

    .address-input.has-error:focus {
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-error), transparent 85%);
    }

    .address-input:disabled {
        opacity: 0.6;
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

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
    }
</style>
