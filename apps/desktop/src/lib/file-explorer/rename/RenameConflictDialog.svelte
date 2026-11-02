<script lang="ts">
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { formatByteSize, formatExactBytes, formatSizePair } from '#lib/units/index.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import DateLabel from '#lib/ui/DateLabel.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { ToolInitiator } from '#lib/ui/started-elsewhere-guard.js'
    import type { ConflictFileInfo, RenameConflictResolution } from './rename-operations'

    interface Props {
        /** The file being renamed (source) */
        renamedFile: ConflictFileInfo
        /** The existing file that would be overwritten */
        existingFile: ConflictFileInfo
        /**
         * Whether the clobbered file has a Trash to go to (`paneOffersTrash`).
         *
         * False hides that button and leaves the permanent overwrite as the only
         * way through, Enter included. ❗ Offering it anyway is not a cosmetic
         * mistake: `moveToTrash` is refused on a volume serving its own I/O and
         * inside an archive, and the rename chained behind it is dropped, so the
         * dialog closes having done nothing at all.
         */
        supportsTrash: boolean
        /** Who started the rename, when it was a guarded caller: they chose the name this ran into. */
        startedBy?: ToolInitiator
        onResolve: (resolution: RenameConflictResolution) => void
    }

    const { renamedFile, existingFile, supportsTrash, startedBy, onResolve }: Props = $props()

    /**
     * A folder holds the name. ❗ No permanent overwrite then: replacing it would take
     * everything inside, and every backend refuses a confirmed replace of a folder that
     * holds something anyway (`Volume::rename`). The trash takes it whole, recoverably.
     */
    const existingIsFolder = $derived(existingFile.isDirectory)

    /** What Enter means here: overwrite, reversibly where the volume allows it, and never a folder for good. */
    const enterResolution = $derived<RenameConflictResolution>(
        supportsTrash ? 'overwrite-trash' : existingIsFolder ? 'continue' : 'overwrite-delete',
    )

    // Group A wire-format: IPC may send `null` for modifiedAt; accept both null and undefined.
    const renamedIsNewer = $derived(
        renamedFile.modifiedAt != null &&
            existingFile.modifiedAt != null &&
            renamedFile.modifiedAt > existingFile.modifiedAt,
    )
    // A folder's size is no size at all (its listing says 0), so sizes only compare between two files.
    const sizesCompare = $derived(!renamedFile.isDirectory && !existingFile.isDirectory)
    const renamedIsLarger = $derived(sizesCompare && renamedFile.size > existingFile.size)
    const existingIsLarger = $derived(sizesCompare && existingFile.size > renamedFile.size)
    /** Worded as a pair, so two files a few bytes apart never both read "1.18 MiB"
     *  (exact bytes then), and equal ones say so. A folder side shows no size. */
    const sizePair = $derived(sizesCompare ? formatSizePair(renamedFile.size, existingFile.size) : null)
    const renamedSizeText = $derived(sizePair?.first.text ?? formatByteSize(renamedFile.size))
    const existingSizeText = $derived(sizePair?.second.text ?? formatByteSize(existingFile.size))
    const sameSize = $derived(sizePair?.same ?? false)

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            event.preventDefault()
            onResolve(enterResolution)
        }
    }
</script>

<ModalDialog
    titleId="rename-conflict-title"
    dialogId="rename-conflict"
    {startedBy}
    role="alertdialog"
    ariaDescribedby="rename-conflict-description"
    onkeydown={handleKeydown}
    onclose={() => {
        onResolve('continue')
    }}
    containerStyle="min-width: 440px; max-width: 520px"
>
    {#snippet title()}{tString(
            existingIsFolder ? 'fileExplorer.renameConflict.titleFolder' : 'fileExplorer.renameConflict.title',
        )}{/snippet}

    <p id="rename-conflict-description" class="description">
        {tString('fileExplorer.renameConflict.description', { name: existingFile.name })}
    </p>

    <div class="file-comparison">
        <div class="file-card">
            <div class="file-card-header">{tString('fileExplorer.renameConflict.yours', { name: renamedFile.name })}</div>
            <div class="file-card-body">
                {#if !renamedFile.isDirectory}
                    <div class="file-meta">
                        <span class="meta-label">{tString('fileExplorer.renameConflict.size')}</span>
                        <span class="meta-value">
                            <span
                                class="size-value"
                                class:newer={renamedIsLarger}
                                use:tooltip={formatExactBytes(renamedFile.size)}>{renamedSizeText}</span
                            >{#if sameSize}
                                <span class="same-size">{tString('common.sizeComparison.same')}</span>{/if}
                        </span>
                    </div>
                {/if}
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
                {#if !existingFile.isDirectory}
                    <div class="file-meta">
                        <span class="meta-label">{tString('fileExplorer.renameConflict.size')}</span>
                        <span class="meta-value">
                            <span
                                class="size-value"
                                class:newer={existingIsLarger}
                                use:tooltip={formatExactBytes(existingFile.size)}>{existingSizeText}</span
                            >
                        </span>
                    </div>
                {/if}
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

    <!-- The ways out sit left, the two overwrites right with Enter's (the primary) last. When
         a long translation can't fit both pairs on one line, the overwrites drop to a row of
         their own, and the panel widens to keep them on one line (`ModalDialog`'s footer floor). -->
    {#snippet footerLeading()}
        <div class="leading-buttons">
            <Button
                variant="secondary"
                onclick={() => {
                    onResolve('cancel')
                }}>{tString('fileExplorer.renameConflict.cancel')}</Button
            >
            <Button
                variant={enterResolution === 'continue' ? 'primary' : 'secondary'}
                onclick={() => {
                    onResolve('continue')
                }}>{tString('fileExplorer.renameConflict.continueRenaming')}</Button
            >
        </div>
    {/snippet}

    {#snippet footer()}
        {#if !existingIsFolder}
            <Button
                variant="danger"
                onclick={() => {
                    onResolve('overwrite-delete')
                }}>{tString('fileExplorer.renameConflict.overwriteDelete')}</Button
            >
        {/if}
        {#if supportsTrash}
            <Button
                variant="primary"
                onclick={() => {
                    onResolve('overwrite-trash')
                }}
                >{tString(
                    existingIsFolder
                        ? 'fileExplorer.renameConflict.overwriteTrashFolder'
                        : 'fileExplorer.renameConflict.overwriteTrash',
                )}</Button
            >
        {/if}
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding-bottom: var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    /* The body's last section: the footer's own inset supplies the gap below it. */
    .file-comparison {
        display: flex;
        gap: var(--spacing-md);
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

    .meta-value.newer,
    .size-value.newer {
        color: var(--color-allow);
        font-weight: 600;
    }

    .same-size {
        color: var(--color-text-secondary);
    }

    .leading-buttons {
        display: flex;
        gap: var(--spacing-md);
        white-space: nowrap;
    }
</style>
