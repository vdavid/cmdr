<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import {
        createDirectory,
        findFileIndex,
        getAiStatus,
        getFileAt,
        getFolderSuggestions,
        listen,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import type { DirectoryDiff } from '$lib/file-explorer/types'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

    interface Props {
        /** The directory in which to create the new folder */
        currentPath: string
        /** Listing ID of the current directory (for conflict checking) */
        listingId: string
        /** Whether hidden files are shown (affects index lookups) */
        showHiddenFiles: boolean
        /** Pre-fill name (filename without extension, or empty) */
        initialName: string
        /** Volume ID for the filesystem (e.g., "root" for local, "mtp-336592896:65537" for MTP) */
        volumeId?: string
        onCreated: (folderName: string) => void
        onCancel: () => void
    }

    const { currentPath, listingId, showHiddenFiles, initialName, volumeId, onCreated, onCancel }: Props = $props()

    let folderName = $state(initialName)
    let errorMessage = $state('')
    let isChecking = $state(false)
    let nameInputRef: HTMLInputElement | undefined = $state()
    let unlistenDiff: UnlistenFn | undefined

    // AI suggestions - start with null to indicate "checking", then true/false once known
    let aiAvailable = $state<boolean | null>(null)
    let aiSuggestions = $state<string[]>([])
    let aiLoading = $state(false)

    // Debounce timer for validation
    let validateTimer: ReturnType<typeof setTimeout> | undefined

    const currentDirName = $derived(currentPath.split('/').pop() || currentPath)
    const isValid = $derived(folderName.trim().length > 0 && !errorMessage)

    async function validateName(name: string) {
        const trimmed = name.trim()
        if (trimmed === '') {
            errorMessage = ''
            return
        }
        if (trimmed.includes('/') || trimmed.includes('\0')) {
            errorMessage = 'Folder name contains invalid characters.'
            return
        }

        isChecking = true
        try {
            const index = await findFileIndex(listingId, trimmed, showHiddenFiles)
            if (index !== null) {
                const entry = await getFileAt(listingId, index, showHiddenFiles)
                if (entry?.isDirectory) {
                    errorMessage = 'There is already a folder by this name in this folder.'
                } else {
                    errorMessage = 'There is already a file by this name in this folder.'
                }
            } else {
                errorMessage = ''
            }
        } catch {
            // If lookup fails (listing gone), clear error and let the backend handle it
            errorMessage = ''
        } finally {
            isChecking = false
        }
    }

    function scheduleValidation() {
        if (validateTimer) clearTimeout(validateTimer)
        validateTimer = setTimeout(() => {
            void validateName(folderName)
        }, 100)
    }

    onMount(async () => {
        await tick()
        nameInputRef?.focus()
        nameInputRef?.select()

        // Validate initial name if pre-filled
        if (folderName.trim()) {
            void validateName(folderName)
        }

        // Fetch AI suggestions if AI is available
        void fetchAiSuggestions()

        // Listen for directory changes to re-validate.
        // Small delay ensures the listing cache is fully consistent after the diff is applied.
        unlistenDiff = await listen<DirectoryDiff>('directory-diff', (event) => {
            if (event.payload.listingId !== listingId) return
            scheduleValidation()
        })
    })

    onDestroy(() => {
        if (validateTimer) clearTimeout(validateTimer)
        unlistenDiff?.()
    })

    async function fetchAiSuggestions() {
        try {
            const status = await getAiStatus()
            if (status !== 'available') {
                aiAvailable = false
                return
            }

            aiAvailable = true
            aiLoading = true
            aiSuggestions = await getFolderSuggestions(listingId, currentPath, showHiddenFiles)
        } catch {
            // Graceful degradation — hide suggestions on error
            aiSuggestions = []
        } finally {
            aiLoading = false
        }
    }

    function selectSuggestion(name: string) {
        folderName = name
        scheduleValidation()
        nameInputRef?.focus()
    }

    async function handleConfirm() {
        const trimmed = folderName.trim()
        if (!trimmed || errorMessage) return
        try {
            await createDirectory(currentPath, trimmed, volumeId)
            onCreated(trimmed)
        } catch (e) {
            errorMessage = String(e)
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleConfirm()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            void handleConfirm()
        }
    }

    function handleInput() {
        scheduleValidation()
    }
</script>

<ModalDialog
    titleId="new-folder-title"
    onkeydown={handleKeydown}
    dialogId="mkdir-confirmation"
    onclose={onCancel}
    containerStyle="width: 400px"
>
    {#snippet title()}New folder{/snippet}

    <div class="dialog-body">
        <p class="subtitle">Create folder in <span class="dir-name">{currentDirName}</span></p>

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
                placeholder="Example: my-project"
                onkeydown={handleInputKeydown}
                oninput={handleInput}
            />
            {#if errorMessage}
                <p id="new-folder-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

        {#if aiAvailable !== false}
            <div class="ai-suggestions" aria-label="AI suggestions">
                <span class="ai-suggestions-header">AI suggestions:</span>
                {#if aiAvailable === null || aiLoading}
                    <span class="ai-suggestions-loading">Loading...</span>
                {:else if aiSuggestions.length > 0}
                    <ul role="list">
                        {#each aiSuggestions as suggestion (suggestion)}
                            <li role="listitem">
                                <button
                                    type="button"
                                    class="suggestion-item"
                                    onclick={() => {
                                        selectSuggestion(suggestion)
                                    }}
                                >
                                    {suggestion}
                                </button>
                            </li>
                        {/each}
                    </ul>
                {:else}
                    <span class="ai-suggestions-empty">No suggestions</span>
                {/if}
            </div>
        {/if}

        <div class="button-row">
            <button class="secondary" onclick={onCancel}>Cancel</button>
            <button class="primary" onclick={() => void handleConfirm()} disabled={!isValid || isChecking}>OK</button>
        </div>
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 24px 20px;
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
        font-family: var(--font-system) sans-serif;
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

    .ai-suggestions {
        margin-bottom: 16px;
        min-height: 52px;
        text-align: center;
    }

    .ai-suggestions-header {
        display: block;
        font-size: 12px;
        font-weight: 500;
        color: var(--color-text-secondary);
        margin-bottom: 6px;
    }

    .ai-suggestions-loading,
    .ai-suggestions-empty {
        font-size: 12px;
        color: var(--color-text-muted);
        font-style: italic;
    }

    .ai-suggestions ul {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        gap: 6px;
    }

    .suggestion-item {
        padding: 4px 10px;
        font-size: 12px;
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-primary);
        border-radius: 4px;
        cursor: pointer;
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .suggestion-item:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border-color: var(--color-accent);
    }

    .suggestion-item:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
