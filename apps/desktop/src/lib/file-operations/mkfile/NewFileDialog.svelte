<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import { createFile, findFileIndex, getFileAt, isIpcError, listen, type UnlistenFn } from '$lib/tauri-commands'
    import { validateDisallowedChars, validateNameLength, validatePathLength } from '$lib/utils/filename-validation'
    import type { DirectoryDiff } from '$lib/file-explorer/types'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        /** The directory in which to create the new file */
        currentPath: string
        /** Listing ID of the current directory (for conflict checking) */
        listingId: string
        /** Whether hidden files are shown (affects index lookups) */
        showHiddenFiles: boolean
        /** Pre-fill name (full filename with extension, or empty) */
        initialName: string
        /** Volume ID for the filesystem (like "root" for local, "mtp-336592896:65537" for MTP) */
        volumeId: string
        onCreated: (fileName: string) => void
        onCancel: () => void
    }

    const { currentPath, listingId, showHiddenFiles, initialName, volumeId, onCreated, onCancel }: Props = $props()

    let fileName = $state(initialName)
    let errorMessage = $state('')
    let isChecking = $state(false)
    let nameInputRef: HTMLInputElement | undefined = $state()
    let unlistenDiff: UnlistenFn | undefined

    // Debounce timer for validation
    let validateTimer: ReturnType<typeof setTimeout> | undefined

    const currentDirName = $derived(currentPath.split('/').pop() || currentPath)
    const isValid = $derived(fileName.trim().length > 0 && !errorMessage)

    async function validateName(name: string) {
        const trimmed = name.trim()
        if (trimmed === '') {
            errorMessage = ''
            return
        }

        // Sync validators: chars, name length, full path length
        const charCheck = validateDisallowedChars(trimmed, true)
        if (charCheck.severity === 'error') {
            errorMessage = charCheck.message
            return
        }
        const nameLenCheck = validateNameLength(trimmed, true)
        if (nameLenCheck.severity === 'error') {
            errorMessage = nameLenCheck.message
            return
        }
        const pathLenCheck = validatePathLength(currentPath, trimmed)
        if (pathLenCheck.severity === 'error') {
            errorMessage = pathLenCheck.message
            return
        }

        // Sync checks passed — clear any previous error, then run async conflict check
        errorMessage = ''

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
            void validateName(fileName)
        }, 100)
    }

    onMount(async () => {
        await tick()
        nameInputRef?.focus()
        nameInputRef?.select()

        // Validate initial name if pre-filled
        if (fileName.trim()) {
            void validateName(fileName)
        }

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

    async function handleConfirm() {
        const trimmed = fileName.trim()
        if (!trimmed || errorMessage) return
        try {
            await createFile(currentPath, trimmed, volumeId)
            onCreated(trimmed)
        } catch (e) {
            errorMessage = isIpcError(e) ? e.message : String(e)
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
    titleId="new-file-title"
    onkeydown={handleKeydown}
    dialogId="new-file-confirmation"
    onclose={onCancel}
    containerStyle="width: 400px"
>
    {#snippet title()}New file{/snippet}

    <div class="dialog-body">
        <p class="subtitle">Create file in <span class="dir-name">{currentDirName}</span></p>

        <div class="input-group">
            <input
                bind:this={nameInputRef}
                bind:value={fileName}
                type="text"
                class="name-input"
                class:has-error={!!errorMessage}
                aria-label="File name"
                aria-describedby={errorMessage ? 'new-file-error' : undefined}
                aria-invalid={!!errorMessage}
                spellcheck="false"
                autocomplete="off"
                placeholder="Example: notes.txt"
                onkeydown={handleInputKeydown}
                oninput={handleInput}
            />
            {#if errorMessage}
                <p id="new-file-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

        <div class="button-row">
            <Button variant="secondary" onclick={onCancel}>Cancel</Button>
            <Button variant="primary" onclick={() => void handleConfirm()} disabled={!isValid || isChecking}>OK</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .subtitle {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        text-align: center;
    }

    .dir-name {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .name-input {
        width: 100%;
        padding: var(--spacing-md) var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .name-input.has-error {
        border-color: var(--color-error);
    }

    .name-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .name-input:focus {
        outline: none;
        box-shadow: var(--shadow-focus);
    }

    .name-input.has-error:focus {
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-error), transparent 85%);
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
