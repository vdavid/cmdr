<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { ConflictFileInfo, RenameConflictResolution } from './rename-operations'

    interface Props {
        /** The file being renamed (source) */
        renamedFile: ConflictFileInfo
        /** The existing file that would be overwritten */
        existingFile: ConflictFileInfo
        onResolve: (resolution: RenameConflictResolution) => void
    }

    const { renamedFile, existingFile, onResolve }: Props = $props()

    // Group A wire-format: IPC may send `null` for modifiedAt; accept both null and undefined.
    const renamedIsNewer = $derived(
        renamedFile.modifiedAt != null &&
            existingFile.modifiedAt != null &&
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
    {#snippet title()}{tString('fileExplorer.renameConflict.title')}{/snippet}

    <p id="rename-conflict-description" class="description">
        {tString('fileExplorer.renameConflict.description', { name: existingFile.name })}
    </p>

    <div class="file-comparison">
        <div class="file-card">
            <div class="file-card-header">{tString('fileExplorer.renameConflict.yours', { name: renamedFile.name })}</div>
            <div class="file-card-body">
                <div class="file-meta">
                    <span class="meta-label">{tString('fileExplorer.renameConflict.size')}</span>
                    <span class="meta-value" class:newer={renamedIsLarger}>{formatFileSize(renamedFile.size)}</span>
                </div>
                <div class="file-meta">
                    <span class="meta-label">{tString('fileExplorer.renameConflict.modified')}</span>
                    <span class="meta-value" class:newer={renamedIsNewer}>
                        <DateLabel modifiedAt={renamedFile.modifiedAt} />
                    </span>
                </div>
            </div>
        </div>
        <div class="file-card">
            <div class="file-card-header">{tString('fileExplorer.renameConflict.existing', { name: existingFile.name })}</div>
            <div class="file-card-body">
                <div class="file-meta">
                    <span class="meta-label">{tString('fileExplorer.renameConflict.size')}</span>
                    <span class="meta-value" class:newer={!renamedIsLarger && renamedFile.size !== existingFile.size}
                        >{formatFileSize(existingFile.size)}</span
                    >
                </div>
                <div class="file-meta">
                    <span class="meta-label">{tString('fileExplorer.renameConflict.modified')}</span>
                    <span
                        class="meta-value"
                        class:newer={!renamedIsNewer &&
                            renamedFile.modifiedAt !== existingFile.modifiedAt &&
                            existingFile.modifiedAt != null}
                    >
                        <DateLabel modifiedAt={existingFile.modifiedAt} />
                    </span>
                </div>
            </div>
        </div>
    </div>

    <div class="button-row">
        <Button
            variant="primary"
            onclick={() => {
                onResolve('overwrite-trash')
            }}>{tString('fileExplorer.renameConflict.overwriteTrash')}</Button
        >
        <Button
            variant="danger"
            onclick={() => {
                onResolve('overwrite-delete')
            }}>{tString('fileExplorer.renameConflict.overwriteDelete')}</Button
        >
    </div>
    <div class="button-row secondary-row">
        <Button
            variant="secondary"
            onclick={() => {
                onResolve('cancel')
            }}>{tString('fileExplorer.renameConflict.cancel')}</Button
        >
        <Button
            variant="secondary"
            onclick={() => {
                onResolve('continue')
            }}>{tString('fileExplorer.renameConflict.continueRenaming')}</Button
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
        gap: var(--spacing-md);
        padding: 0 var(--spacing-xl) var(--spacing-lg);
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
        gap: var(--spacing-sm);
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
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) var(--spacing-sm);
    }

    .secondary-row {
        padding-bottom: var(--spacing-xl);
    }
</style>
