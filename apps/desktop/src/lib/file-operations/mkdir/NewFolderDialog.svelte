<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import {
        createDirectory,
        findFileIndex,
        getAiStatus,
        getFileAt,
        isIpcError,
        onDirectoryDiff,
        refreshListing,
        streamFolderSuggestions,
        type FolderSuggestionsStream,
        type UnlistenFn,
    } from '$lib/tauri-commands'
    import { validateDisallowedChars, validateNameLength, validatePathLength } from '$lib/utils/filename-validation'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The directory in which to create the new folder */
        currentPath: string
        /** Listing ID of the current directory (for conflict checking) */
        listingId: string
        /** Whether hidden files are shown (affects index lookups) */
        showHiddenFiles: boolean
        /** Pre-fill name (filename without extension, or empty) */
        initialName: string
        /** Volume ID for the filesystem (like "root" for local, "mtp-336592896:65537" for MTP) */
        volumeId?: string
        onCreated: (folderName: string) => void
        onCancel: () => void
    }

    const { currentPath, listingId, showHiddenFiles, initialName, volumeId, onCreated, onCancel }: Props = $props()

    let folderName = $state(initialName)
    let errorMessage = $state('')
    let timeoutError = $state(false)
    let isChecking = $state(false)
    let nameInputRef: HTMLInputElement | undefined = $state()
    let unlistenDiff: UnlistenFn | undefined

    // AI suggestions - start with null to indicate "checking", then true/false once known
    let aiAvailable = $state<boolean | null>(null)
    let aiSuggestions = $state<string[]>([])
    let aiStreaming = $state(false)
    let suggestionsStream: FolderSuggestionsStream | undefined

    // Debounce timer for validation
    let validateTimer: ReturnType<typeof setTimeout> | undefined

    const currentDirName = $derived(currentPath.split('/').pop() || currentPath)
    const isValid = $derived(folderName.trim().length > 0 && !errorMessage && !timeoutError)

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

        // Sync checks passed: clear any previous error, then run async conflict check
        errorMessage = ''

        isChecking = true
        try {
            const index = await findFileIndex(listingId, trimmed, showHiddenFiles)
            if (index !== null) {
                const entry = await getFileAt(listingId, index, showHiddenFiles)
                if (entry?.isDirectory) {
                    errorMessage = tString('fileOperations.shared.conflictExistsFolder')
                } else {
                    errorMessage = tString('fileOperations.shared.conflictExistsFile')
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
        unlistenDiff = await onDirectoryDiff((payload) => {
            if (payload.listingId !== listingId) return
            scheduleValidation()
        })
    })

    onDestroy(() => {
        if (validateTimer) clearTimeout(validateTimer)
        unlistenDiff?.()
        // Cancel the in-flight stream. Tauri 2's `Channel::send` is fire-and-forget;
        // without this explicit signal the backend would keep streaming after the dialog
        // closes, billing cloud providers and pegging local-LLM compute.
        void suggestionsStream?.cancel()
    })

    async function fetchAiSuggestions() {
        try {
            const status = await getAiStatus()
            if (status !== 'available') {
                aiAvailable = false
                return
            }
            aiAvailable = true
            aiSuggestions = []
            aiStreaming = true

            suggestionsStream = streamFolderSuggestions(
                listingId,
                currentPath,
                showHiddenFiles,
                (event) => {
                    switch (event.type) {
                        case 'suggestion':
                            aiSuggestions = [...aiSuggestions, event.name]
                            break
                        case 'done':
                        case 'cancelled':
                        case 'failed':
                            aiStreaming = false
                            break
                    }
                },
            )
            await suggestionsStream.promise
        } catch {
            // Graceful degradation: hide suggestions on error
            aiSuggestions = []
            aiStreaming = false
        }
    }

    function selectSuggestion(name: string) {
        folderName = name
        scheduleValidation()
        nameInputRef?.focus()
    }

    async function handleConfirm() {
        const trimmed = folderName.trim()
        if (!trimmed || errorMessage || timeoutError) return
        try {
            await createDirectory(currentPath, trimmed, volumeId)
            onCreated(trimmed)
        } catch (e) {
            if (isIpcError(e) && e.timedOut) {
                timeoutError = true
                errorMessage = ''
            } else {
                errorMessage = isIpcError(e) ? e.message : String(e)
            }
        }
    }

    function handleRefreshListing() {
        void refreshListing(listingId)
        onCancel()
    }

    function handleTimeoutDismiss() {
        timeoutError = false
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
    {#snippet title()}{tString('fileOperations.mkdir.title')}{/snippet}

    <div class="dialog-body">
        <p class="subtitle">
            <Trans key="fileOperations.mkdir.createIn" params={{ name: currentDirName }} snippets={{ dir }} />
        </p>

        <div class="input-group">
            <input
                bind:this={nameInputRef}
                bind:value={folderName}
                type="text"
                class="name-input"
                class:has-error={!!errorMessage}
                aria-label={tString('fileOperations.mkdir.nameAria')}
                aria-describedby={errorMessage ? 'new-folder-error' : undefined}
                aria-invalid={!!errorMessage}
                spellcheck="false"
                autocomplete="off"
                placeholder={tString('fileOperations.mkdir.placeholder')}
                onkeydown={handleInputKeydown}
                oninput={handleInput}
            />
            {#if errorMessage}
                <p id="new-folder-error" class="error-message" role="alert">{errorMessage}</p>
            {/if}
        </div>

        {#if timeoutError}
            <div class="timeout-warning" role="alert">
                <p class="timeout-message">
                    {tString('fileOperations.mkdir.timeoutMessage')}
                </p>
                <div class="timeout-actions">
                    <Button size="mini" onclick={handleRefreshListing}
                        >{tString('fileOperations.mkdir.timeoutRefresh')}</Button
                    >
                    <Button size="mini" onclick={handleTimeoutDismiss}
                        >{tString('fileOperations.mkdir.timeoutDismiss')}</Button
                    >
                </div>
            </div>
        {/if}

        {#if aiAvailable !== false}
            <div class="ai-suggestions" aria-label={tString('fileOperations.mkdir.aiSuggestionsAria')}>
                <span class="ai-suggestions-header">{tString('fileOperations.mkdir.aiSuggestionsHeader')}</span>
                {#if aiSuggestions.length > 0}
                    <ul role="list" aria-live="polite" aria-relevant="additions">
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
                        {#if aiStreaming}
                            <li role="listitem" aria-hidden="true">
                                <span class="suggestion-item suggestion-pending">…</span>
                            </li>
                        {/if}
                    </ul>
                {:else if aiStreaming}
                    <span class="suggestion-item suggestion-pending" aria-hidden="true">…</span>
                {/if}
            </div>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant="primary" onclick={() => void handleConfirm()} disabled={!isValid || isChecking}
            >{tString('fileOperations.button.ok')}</Button
        >
    {/snippet}
</ModalDialog>

{#snippet dir(children: import('svelte').Snippet)}<span class="dir-name">{@render children()}</span>{/snippet}

<style>
    .dialog-body {
        padding: 0 var(--spacing-xl);
    }

    .subtitle {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
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

    .timeout-warning {
        margin-bottom: var(--spacing-lg);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-sm);
    }

    .timeout-message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-warning);
        line-height: 1.4;
    }

    .timeout-actions {
        display: flex;
        gap: var(--spacing-sm);
        justify-content: flex-end;
    }

    .ai-suggestions {
        margin-bottom: var(--spacing-lg);
        min-height: 52px;
        text-align: center;
    }

    .ai-suggestions-header {
        display: block;
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
        margin-bottom: var(--spacing-sm);
    }

    .ai-suggestions ul {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        gap: var(--spacing-sm);
    }

    .suggestion-item {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        transition: all var(--transition-base);
    }

    .suggestion-item:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border-color: var(--color-accent);
    }

    .suggestion-item:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }

    /* Trailing pulsing chip while suggestions are still streaming. Matches the regular
       chip dimensions so the list doesn't reflow on completion. */
    .suggestion-pending {
        animation: suggestion-pulse 1.2s ease-in-out infinite;
        opacity: 0.5;
        pointer-events: none;
        cursor: default;
    }

    @keyframes suggestion-pulse {
        50% {
            opacity: 0.3;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .suggestion-pending {
            animation: none;
            opacity: 0.4;
        }
    }
</style>
