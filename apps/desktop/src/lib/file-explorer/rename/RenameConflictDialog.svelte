<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { formatDateTime, formatFileSize } from '$lib/settings/reactive-settings.svelte'

    export interface ConflictFileInfo {
        name: string
        size: number
        /** Unix timestamp in seconds, or undefined if unavailable */
        modifiedAt: number | undefined
    }

    export type ConflictResolution = 'overwrite-trash' | 'overwrite-delete' | 'cancel' | 'continue'

    interface Props {
        /** The file being renamed (source) */
        renamedFile: ConflictFileInfo
        /** The existing file that would be overwritten */
        existingFile: ConflictFileInfo
        onResolve: (resolution: ConflictResolution) => void
    }

    const { renamedFile, existingFile, onResolve }: Props = $props()

    const renamedIsNewer = $derived(
        renamedFile.modifiedAt !== undefined &&
            existingFile.modifiedAt !== undefined &&
            renamedFile.modifiedAt > existingFile.modifiedAt,
    )
    const renamedIsLarger = $derived(renamedFile.size > existingFile.size)

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            onResolve('overwrite-trash')
        }
    }
</script>

<ModalDialog
    titleId="rename-conflict-title"
    dialogId="rename-conflict"
    role="alertdialog"
    ariaDescribedby="rename-conflict-description"
    onkeydown={handleKeydown}
    onclose={() => {
        onResolve('continue')
    }}
    containerStyle="min-width: 440px; max-width: 520px"
>
    {#snippet title()}A file with this name already exists{/snippet}

    <p id="rename-conflict-description" class="description">
        "{existingFile.name}" already exists in this folder. What would you like to do?
    </p>

    <div class="file-comparison">
        <div class="file-card">
            <div class="file-card-header">{renamedFile.name} (yours)</div>
            <div class="file-card-body">
                <div class="file-meta">
                    <span class="meta-label">Size</span>
                    <span class="meta-value" class:newer={renamedIsLarger}>{formatFileSize(renamedFile.size)}</span>
                </div>
                <div class="file-meta">
                    <span class="meta-label">Modified</span>
                    <span class="meta-value" class:newer={renamedIsNewer}>{formatDateTime(renamedFile.modifiedAt)}</span
                    >
                </div>
            </div>
        </div>
        <div class="file-card">
            <div class="file-card-header">{existingFile.name} (existing)</div>
            <div class="file-card-body">
                <div class="file-meta">
                    <span class="meta-label">Size</span>
                    <span class="meta-value" class:newer={!renamedIsLarger && renamedFile.size !== existingFile.size}
                        >{formatFileSize(existingFile.size)}</span
                    >
                </div>
                <div class="file-meta">
                    <span class="meta-label">Modified</span>
                    <span
                        class="meta-value"
                        class:newer={!renamedIsNewer &&
                            renamedFile.modifiedAt !== existingFile.modifiedAt &&
                            existingFile.modifiedAt !== undefined}>{formatDateTime(existingFile.modifiedAt)}</span
                    >
                </div>
            </div>
        </div>
    </div>

    <div class="button-row">
        <Button
            variant="primary"
            onclick={() => {
                onResolve('overwrite-trash')
            }}>Overwrite and trash old file</Button
        >
        <Button
            variant="danger"
            onclick={() => {
                onResolve('overwrite-delete')
            }}>Overwrite and delete old file</Button
        >
    </div>
    <div class="button-row secondary-row">
        <Button
            variant="secondary"
            onclick={() => {
                onResolve('cancel')
            }}>Cancel</Button
        >
        <Button
            variant="secondary"
            onclick={() => {
                onResolve('continue')
            }}>Continue renaming</Button
        >
    </div>
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .file-comparison {
        display: flex;
        gap: 12px;
        padding: 0 24px 16px;
    }

    .file-card {
        flex: 1;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        overflow: hidden;
    }

    .file-card-header {
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        text-transform: uppercase;
        letter-spacing: 0.3px;
    }

    .file-card-body {
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .file-meta {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        gap: 8px;
    }

    .meta-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .meta-value {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        font-variant-numeric: tabular-nums;
    }

    .meta-value.newer {
        color: var(--color-allow);
        font-weight: 600;
    }

    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
        padding: 0 24px 8px;
    }

    .secondary-row {
        padding-bottom: 20px;
    }
</style>
