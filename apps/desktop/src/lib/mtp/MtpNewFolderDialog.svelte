<script lang="ts">
    /**
     * Dialog to create a new folder on an MTP device.
     */
    import { onMount, tick } from 'svelte'

    interface Props {
        /** Current folder name for context */
        currentFolderName: string
        /** Existing names in the current folder (for conflict checking) */
        existingNames: string[]
        /** Callback when folder is created */
        onConfirm: (folderName: string) => void
        /** Callback when user cancels */
        onCancel: () => void
    }

    const { currentFolderName, existingNames, onConfirm, onCancel }: Props = $props()

    let folderName = $state('')
    let errorMessage = $state('')
    let overlayElement: HTMLDivElement | undefined = $state()
    let nameInputRef: HTMLInputElement | undefined = $state()

    const isValid = $derived(folderName.trim().length > 0 && !errorMessage)

    function validateName(name: string) {
        const trimmed = name.trim()
        if (trimmed === '') {
            errorMessage = ''
            return
        }
        if (trimmed.includes('/') || trimmed.includes('\0')) {
            errorMessage = 'Folder name contains invalid characters.'
            return
        }
        if (existingNames.some((n) => n.toLowerCase() === trimmed.toLowerCase())) {
            errorMessage = 'An item with this name already exists.'
            return
        }
        errorMessage = ''
    }

    function handleInput() {
        validateName(folderName)
    }

    function handleConfirm() {
        const trimmed = folderName.trim()
        if (!trimmed || errorMessage) return
        onConfirm(trimmed)
    }

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape') {
            onCancel()
        } else if (event.key === 'Enter') {
            handleConfirm()
        }
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()
        await tick()
        nameInputRef?.focus()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-folder-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="new-folder-dialog">
        <h2 id="new-folder-title">New folder</h2>
        <p class="subtitle">Create folder in <span class="dir-name">{currentFolderName}</span></p>

        <div class="input-group">
            <input
                bind:this={nameInputRef}
                bind:value={folderName}
                type="text"
                class="name-input"
                class:has-error={!!errorMessage}
                aria-label="Folder name"
                aria-describedby={errorMessage ? 'new-folder-error' : undefined}
                aria-invalid={!!errorMessage}
                spellcheck="false"
                autocomplete="off"
                placeholder="Example: my-folder"
                oninput={handleInput}
            />
            {#if errorMessage}
                <p id="new-folder-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

        <div class="button-row">
            <button class="secondary" onclick={onCancel}>Cancel</button>
            <button class="primary" onclick={handleConfirm} disabled={!isValid}>OK</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }

    .new-folder-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        width: 400px;
        padding: 20px 24px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 4px;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
    }

    .subtitle {
        margin: 0 0 16px;
        font-size: 13px;
        color: var(--color-text-secondary);
        text-align: center;
    }

    .dir-name {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .input-group {
        margin-bottom: 16px;
    }

    .name-input {
        width: 100%;
        padding: 10px 12px;
        font-size: 13px;
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: 6px;
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .name-input.has-error {
        border-color: var(--color-error);
    }

    .name-input::placeholder {
        color: var(--color-text-muted);
    }

    .name-input:focus {
        outline: none;
        box-shadow: 0 0 0 2px rgba(77, 163, 255, 0.2);
    }

    .name-input.has-error:focus {
        box-shadow: 0 0 0 2px rgba(211, 47, 47, 0.2);
    }

    .error-message {
        margin: 6px 0 0;
        font-size: 12px;
        color: var(--color-error);
    }

    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
    }

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
