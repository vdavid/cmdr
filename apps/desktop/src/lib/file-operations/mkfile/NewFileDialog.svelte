<script lang="ts">
    import { createFile, type Initiator } from '$lib/tauri-commands'
    import { asMutationError } from '$lib/file-operations/mutation-error'
    import { renderMutationError } from '$lib/file-operations/mutation-error-messages'
    import { NewEntryNameCheck } from '$lib/file-operations/new-entry-name-check.svelte'
    import NewEntryNameField from '$lib/file-operations/NewEntryNameField.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

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
        /** Who triggered this create (`aiClient` for the MCP `mkfile` tool). */
        initiator?: Initiator
        onCreated: (fileName: string) => void
        onCancel: () => void
    }

    const { currentPath, listingId, showHiddenFiles, initialName, volumeId, initiator, onCreated, onCancel }: Props =
        $props()

    let fileName = $state(initialName)

    // Name validation + clash lookup; `NewEntryNameField` runs its lifecycle.
    const check = new NewEntryNameCheck({ currentPath, listingId, showHiddenFiles, getName: () => fileName })

    const isValid = $derived(fileName.trim().length > 0 && !check.errorMessage)

    async function handleConfirm() {
        const trimmed = fileName.trim()
        if (!trimmed || check.errorMessage) return
        try {
            await createFile(currentPath, trimmed, volumeId, initiator)
            onCreated(trimmed)
        } catch (e) {
            const failure = asMutationError(e)
            check.errorMessage = failure ? renderMutationError(failure, 'file') : String(e)
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleConfirm()
        }
    }
</script>

<ModalDialog
    titleId="new-file-title"
    onkeydown={handleKeydown}
    dialogId="new-file-confirmation"
    onclose={onCancel}
    containerStyle="width: 400px"
    resizable="horizontal"
>
    {#snippet title()}{tString('fileOperations.mkfile.title')}{/snippet}

    <div class="dialog-body">
        <NewEntryNameField kind="file" {currentPath} {check} bind:value={fileName} onSubmit={() => void handleConfirm()} />
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onCancel}>{tString('fileOperations.button.cancel')}</Button>
        <Button variant="primary" onclick={() => void handleConfirm()} disabled={!isValid || check.isChecking}
            >{tString('fileOperations.button.ok')}</Button
        >
    {/snippet}
</ModalDialog>
