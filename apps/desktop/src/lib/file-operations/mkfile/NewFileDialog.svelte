<script lang="ts">
    import { onDestroy } from 'svelte'
    import { createFile, type Initiator } from '$lib/tauri-commands'
    import { CreateSubmission } from '$lib/file-operations/create-submission.svelte'
    import { NewEntryNameCheck } from '$lib/file-operations/new-entry-name-check.svelte'
    import NewEntryNameField from '$lib/file-operations/NewEntryNameField.svelte'
    import StillCreatingNotice from '$lib/file-operations/StillCreatingNotice.svelte'
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

    // OK to how the create really ended, slow volumes included.
    const submission = new CreateSubmission({
        kind: 'file',
        create: (name, wait) => createFile(currentPath, name, volumeId, initiator, wait),
        onCreated,
        onRefused: (message) => {
            check.errorMessage = message
        },
    })

    // A create still running carries on; its end no longer steers this dialog.
    onDestroy(() => {
        submission.close()
    })

    const isValid = $derived(fileName.trim().length > 0 && !check.errorMessage)

    async function handleConfirm() {
        const trimmed = fileName.trim()
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
    titleId="new-file-title"
    onkeydown={handleKeydown}
    dialogId="new-file-confirmation"
    onclose={onCancel}
    containerStyle="width: 400px"
    resizable="horizontal"
>
    {#snippet title()}{tString('fileOperations.mkfile.title')}{/snippet}

    <div class="dialog-body">
        <NewEntryNameField kind="file" {currentPath} {volumeId} {check} bind:value={fileName} onSubmit={() => void handleConfirm()} />

        {#if submission.phase === 'stillCreating'}
            <StillCreatingNotice name={submission.submittedName} />
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
