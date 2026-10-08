<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import {
        createDirectory,
        getAiStatus,
        streamFolderSuggestions,
        type FolderSuggestionsStream,
        type Initiator,
    } from '$lib/tauri-commands'
    import { CreateSubmission } from '$lib/file-operations/create-submission.svelte'
    import { NewEntryNameCheck } from '$lib/file-operations/new-entry-name-check.svelte'
    import NewEntryNameField from '$lib/file-operations/NewEntryNameField.svelte'
    import StillCreatingNotice from '$lib/file-operations/StillCreatingNotice.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting } from '$lib/settings'
    import { cloudAiBlocked, refreshCloudConsent } from '$lib/ai/cloud-consent.svelte'

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
        /** Who triggered this create (`aiClient` for the MCP `mkdir` tool). */
        initiator?: Initiator
        onCreated: (folderName: string) => void
        onCancel: () => void
    }

    const { currentPath, listingId, showHiddenFiles, initialName, volumeId, initiator, onCreated, onCancel }: Props =
        $props()

    let folderName = $state(initialName)
    let nameInputRef: HTMLInputElement | undefined = $state()

    // Name validation + clash lookup; `NewEntryNameField` runs its lifecycle.
    const check = new NewEntryNameCheck({ currentPath, listingId, showHiddenFiles, getName: () => folderName })

    // OK to how the create really ended, slow volumes included.
    const submission = new CreateSubmission({
        kind: 'folder',
        create: (name, wait) => createDirectory(currentPath, name, volumeId, initiator, wait),
        onCreated,
        onRefused: (message) => {
            check.errorMessage = message
        },
    })

    // AI suggestions - start with null to indicate "checking", then true/false once known
    let aiAvailable = $state<boolean | null>(null)
    let aiSuggestions = $state<string[]>([])
    let aiStreaming = $state(false)
    let suggestionsStream: FolderSuggestionsStream | undefined

    const isValid = $derived(folderName.trim().length > 0 && !check.errorMessage)

    onMount(() => {
        // Fetch AI suggestions if AI is available
        void fetchAiSuggestions()
    })

    onDestroy(() => {
        // A create still running carries on; its end no longer steers this dialog.
        submission.close()
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
            // On Cloud without "Allow cloud AI", stay quiet: no suggestions, no copy. The backend
            // refuses anyway (an empty `done`); skipping the call spares the round trip.
            const provider = getSetting('ai.provider')
            if (provider === 'cloud') {
                await refreshCloudConsent()
                if (cloudAiBlocked(provider)) {
                    aiAvailable = false
                    return
                }
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
        check.schedule()
        nameInputRef?.focus()
    }

    async function handleConfirm() {
        const trimmed = folderName.trim()
        if (!trimmed || check.errorMessage) return
        await submission.submit(trimmed)
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleConfirm()
        }
    }

</script>

<ModalDialog
    titleId="new-folder-title"
    onkeydown={handleKeydown}
    dialogId="mkdir-confirmation"
    onclose={onCancel}
    containerStyle="width: 400px"
    resizable="horizontal"
>
    {#snippet title()}{tString('fileOperations.mkdir.title')}{/snippet}

    <div class="dialog-body">
        <NewEntryNameField
            kind="folder"
            {currentPath}
            {volumeId}
            {check}
            bind:value={folderName}
            bind:inputElement={nameInputRef}
            onSubmit={() => void handleConfirm()}
        />

        {#if submission.phase === 'stillCreating'}
            <StillCreatingNotice name={submission.submittedName} />
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
                                    use:tooltip={{ text: suggestion, overflowOnly: true }}
                                    onclick={() => {
                                        selectSuggestion(suggestion)
                                    }}
                                >
                                    {suggestion}
                                </button>
                            </li>
                        {/each}
                        {#if aiStreaming}
                            <!-- The `<li>` is already `aria-hidden`, so a screen reader skips this
                                 whole placeholder rather than announcing a nameless empty item; the
                                 span repeats the marker so its own `.suggestion-pending` rule stays
                                 exempt from the opacity check without leaning on the ancestor. -->
                            <li role="listitem" aria-hidden="true">
                                <span class="suggestion-item suggestion-pending" aria-hidden="true">…</span>
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
        <Button variant="secondary" onclick={onCancel}
            >{tString(submission.phase === 'stillCreating' ? 'fileOperations.button.close' : 'fileOperations.button.cancel')}</Button
        >
        <Button
            variant="primary"
            onclick={() => void handleConfirm()}
            disabled={!isValid || check.isChecking || submission.busy}>{tString('fileOperations.button.ok')}</Button
        >
    {/snippet}
</ModalDialog>

<style>
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
