<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
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
        <button
            class="primary"
            onclick={() => {
                onResolve('overwrite-trash')
            }}>Overwrite and trash old file</button
        >
        <button
            class="danger"
            onclick={() => {
                onResolve('overwrite-delete')
            }}>Overwrite and delete old file</button
        >
    </div>
    <div class="button-row secondary-row">
        <button
            class="secondary"
            onclick={() => {
                onResolve('cancel')
            }}>Cancel</button
        >
        <button
            class="secondary"
            onclick={() => {
                onResolve('continue')
            }}>Continue renaming</button
        >
    </div>
</ModalDialog>

<style>
    .description {
        margin: 0;
        padding: 0 24px 16px;
        font-size: 13px;
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
        border: 1px solid var(--color-border-primary);
        border-radius: 8px;
        overflow: hidden;
    }

    .file-card-header {
        padding: 8px 12px;
        font-size: 11px;
        font-weight: 600;
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        text-transform: uppercase;
        letter-spacing: 0.3px;
    }

    .file-card-body {
        padding: 8px 12px;
        display: flex;
        flex-direction: column;
        gap: 6px;
    }

    .file-meta {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        gap: 8px;
    }

    .meta-label {
        font-size: 11px;
        color: var(--color-text-muted);
    }

    .meta-value {
        font-size: 12px;
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

    button {
        padding: 8px 16px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover {
        filter: brightness(1.1);
    }

    .danger {
        background: transparent;
        color: var(--color-error);
        border: 1px solid var(--color-error);
    }

    .danger:hover {
        background: var(--color-error-bg);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
