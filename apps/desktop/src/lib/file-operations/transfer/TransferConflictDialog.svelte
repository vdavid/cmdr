<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { formatDate } from '$lib/file-explorer/selection/selection-info-utils'
    import type { WriteConflictEvent } from '$lib/tauri-commands'
    import type { ConflictResolution } from '$lib/file-explorer/types'

    interface Props {
        /** The conflict to resolve (one clash; the BE re-prompts per remaining clash). */
        conflictEvent: WriteConflictEvent
        /** Operation-type flags, driving the bottom Rollback/Cancel row. */
        isCopy: boolean
        isMove: boolean
        /** A move where source and dest are the SAME non-default volume (no backend
         *  rollback): the Rollback affordance renders disabled with a tooltip. */
        isSameVolumeMove: boolean
        /** Disables the cancel/rollback buttons while a cancel is in flight. */
        isCancelling: boolean
        /** Disables every resolution button while a resolution IPC is in flight. */
        isResolvingConflict: boolean
        /** Resolve this clash (skip/rename/overwrite/…), optionally applying to all. */
        onResolve: (resolution: ConflictResolution, applyToAll: boolean) => void
        /** Back out of the operation. `rollback` reverses already-written files. */
        onCancel: (rollback: boolean) => void
    }

    const { conflictEvent, isCopy, isMove, isSameVolumeMove, isCancelling, isResolvingConflict, onResolve, onCancel }: Props =
        $props()

    /** Returns CSS class for size coloring based on bytes (kb/mb/gb/tb) */
    function getSizeColorClass(bytes: number): string {
        if (bytes < 1024) return 'size-bytes'
        if (bytes < 1024 * 1024) return 'size-kb'
        if (bytes < 1024 * 1024 * 1024) return 'size-mb'
        if (bytes < 1024 * 1024 * 1024 * 1024) return 'size-gb'
        return 'size-tb'
    }

    const ROLLBACK_UNAVAILABLE_TOOLTIP = $derived(
        tString('fileOperations.transferProgress.rollbackUnavailableTooltip'),
    )

    // The same shape — filename, "Existing:" / "New:" rows, the 4×2 button grid,
    // the Rollback row — serves every clash type. Variants differ only in the row
    // labels, the red warning block above the filename for file→folder, and the
    // "Overwrite" button copy in that one case.
    const fileName = $derived(conflictEvent.destinationPath.split('/').pop() ?? '')
    const existingIsNewer = $derived(conflictEvent.destinationIsNewer)
    const newIsNewer = $derived(!existingIsNewer && conflictEvent.sourceModified !== conflictEvent.destinationModified)
    const sizeDiff = $derived(conflictEvent.sizeDifference)
    const existingIsLarger = $derived(sizeDiff !== null && sizeDiff > 0)
    const newIsLarger = $derived(sizeDiff !== null && sizeDiff < 0)
    const sourceIsDir = $derived(conflictEvent.sourceIsDirectory)
    const destIsDir = $derived(conflictEvent.destinationIsDirectory)
    const isTypeMismatch = $derived(sourceIsDir !== destIsDir)
    const isFileOverFolder = $derived(isTypeMismatch && destIsDir)
    const existingLabel = $derived(
        destIsDir
            ? tString('fileOperations.transferProgress.existingFolderLabel')
            : sourceIsDir
              ? tString('fileOperations.transferProgress.existingFileLabel')
              : tString('fileOperations.transferProgress.existingLabel'),
    )
    const newLabel = $derived(
        sourceIsDir
            ? tString('fileOperations.transferProgress.newFolderLabel')
            : isFileOverFolder
              ? tString('fileOperations.transferProgress.newFileLabel')
              : tString('fileOperations.transferProgress.newLabel'),
    )
    const overwriteLabel = $derived(
        isFileOverFolder
            ? tString('fileOperations.transferProgress.conflictOverwriteFolderWithFile')
            : tString('fileOperations.transferProgress.conflictOverwrite'),
    )
    const overwriteAllLabel = $derived(
        isFileOverFolder
            ? tString('fileOperations.transferProgress.conflictOverwriteFoldersWithFiles')
            : tString('fileOperations.transferProgress.conflictOverwriteAll'),
    )
    // Size renders normally when known and substitutes `(unknown)` in muted color
    // when the BE could not look the destination folder size up. The color class
    // and formatted text are precomputed here (guarding the null) so the markup's
    // `{:else}` branch needs no cross-variable narrowing.
    const destSize = $derived(conflictEvent.destinationSize)
    const destSizeUnknown = $derived(destSize === null)
    const destSizeClass = $derived(destSize === null ? '' : getSizeColorClass(destSize))
    const destSizeText = $derived(destSize === null ? '' : formatFileSize(destSize))
    const srcSize = $derived(conflictEvent.sourceSize)
    const srcSizeUnknown = $derived(srcSize === null)
    const srcSizeClass = $derived(srcSize === null ? '' : getSizeColorClass(srcSize))
    const srcSizeText = $derived(srcSize === null ? '' : formatFileSize(srcSize))
    const smallerDisabledTooltip = $derived(
        destSizeUnknown ? tString('fileOperations.transferProgress.smallerDisabledTooltip') : undefined,
    )
</script>

<div class="conflict-section">
    {#if isFileOverFolder}
        <!-- Red warning sits below the title and above the filename.
             The "boring" title is `File already exists`; the destructive
             swap gets called out here so the user can't miss it. -->
        <p class="conflict-warning" role="alert">
            <span class="conflict-warning-icon" aria-hidden="true">
                <Icon name="triangle-alert" size={16} />
            </span>
            <span>
                <Trans key="fileOperations.transferProgress.warningFileOverFolder" snippets={{ strong }} />
            </span>
        </p>
    {/if}

    <!-- Filename -->
    <p class="conflict-filename" use:tooltip={{ text: conflictEvent.destinationPath, overflowOnly: true }}>
        {fileName}
    </p>

    <!-- File comparison: same shape across all variants. Type tags
         (`Existing (file):` / `New (folder):` etc.) flag the mismatch
         without breaking the layout. -->
    <div class="conflict-comparison">
        <div class="conflict-file">
            <span class="conflict-file-label">{existingLabel}</span>
            {#if destSizeUnknown}
                <span class="conflict-file-size unknown"
                    >{tString('fileOperations.transferProgress.sizeUnknown')}</span
                >
            {:else}
                <span class="conflict-file-size {destSizeClass}">{destSizeText}</span>
            {/if}
            {#if existingIsLarger}<span class="conflict-annotation larger"
                    >{tString('fileOperations.transferProgress.annotationLarger')}</span
                >{/if}
            <span class="conflict-file-date"
                >{conflictEvent.destinationModified
                    ? formatDate(conflictEvent.destinationModified)
                    : ''}</span
            >
            {#if existingIsNewer}<span class="conflict-annotation newer"
                    >{tString('fileOperations.transferProgress.annotationNewer')}</span
                >{/if}
        </div>
        <div class="conflict-file">
            <span class="conflict-file-label">{newLabel}</span>
            {#if srcSizeUnknown}
                <span class="conflict-file-size unknown"
                    >{tString('fileOperations.transferProgress.sizeUnknown')}</span
                >
            {:else}
                <span class="conflict-file-size {srcSizeClass}">{srcSizeText}</span>
            {/if}
            {#if newIsLarger}<span class="conflict-annotation larger"
                    >{tString('fileOperations.transferProgress.annotationLarger')}</span
                >{/if}
            <span class="conflict-file-date"
                >{conflictEvent.sourceModified ? formatDate(conflictEvent.sourceModified) : ''}</span
            >
            {#if newIsNewer}<span class="conflict-annotation newer"
                    >{tString('fileOperations.transferProgress.annotationNewer')}</span
                >{/if}
        </div>
    </div>

    <!-- Buttons. Two columns: left = this-item, right = apply-to-all.
         Last row holds the conditional bulk variants: `Overwrite all
         smaller` only works when the destination size is known
         (a folder dest with no index size disables it with a tooltip);
         `Overwrite all older` always stays enabled (mtime is always
         available even for folder destinations). -->
    <div class="conflict-buttons">
        <div class="conflict-buttons-row">
            <Button
                variant="secondary"
                onclick={() => { onResolve('skip', false); }}
                disabled={isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictSkip')}
            </Button>
            <Button
                variant="secondary"
                onclick={() => { onResolve('skip', true); }}
                disabled={isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictSkipAll')}
            </Button>
        </div>
        <div class="conflict-buttons-row">
            <Button
                variant="secondary"
                onclick={() => { onResolve('rename', false); }}
                disabled={isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictRename')}
            </Button>
            <Button
                variant="secondary"
                onclick={() => { onResolve('rename', true); }}
                disabled={isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictRenameAll')}
            </Button>
        </div>
        <div class="conflict-buttons-row">
            <Button
                variant="secondary"
                onclick={() => { onResolve('overwrite', false); }}
                disabled={isResolvingConflict}
            >
                {overwriteLabel}
            </Button>
            <Button
                variant="secondary"
                onclick={() => { onResolve('overwrite', true); }}
                disabled={isResolvingConflict}
            >
                {overwriteAllLabel}
            </Button>
        </div>
        <div class="conflict-buttons-row">
            <span use:tooltip={smallerDisabledTooltip} class="conflict-button-wrap">
                <Button
                    variant="secondary"
                    onclick={() => { onResolve('overwrite_smaller', true); }}
                    disabled={isResolvingConflict || destSizeUnknown}
                >
                    {tString('fileOperations.transferProgress.conflictOverwriteAllSmaller')}
                </Button>
            </span>
            <Button
                variant="secondary"
                onclick={() => { onResolve('overwrite_older', true); }}
                disabled={isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictOverwriteAllOlder')}
            </Button>
        </div>
    </div>

    <!-- Cancel at bottom. Same-volume volume moves have no backend
         rollback, so Rollback is DISABLED (with a tooltip) and a plain
         Cancel sits alongside it so the user can always back out. -->
    <div class="conflict-cancel">
        {#if isSameVolumeMove}
            <button
                class="danger-text"
                onclick={() => { onCancel(false); }}
                disabled={isCancelling || isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictCancel')}
            </button>
            <span use:tooltip={ROLLBACK_UNAVAILABLE_TOOLTIP} class="disabled-button-wrap">
                <button class="danger-text" disabled
                    >{tString('fileOperations.transferProgress.conflictRollback')}</button
                >
            </span>
        {:else if isCopy || isMove}
            <button
                class="danger-text"
                onclick={() => { onCancel(true); }}
                disabled={isCancelling || isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictRollback')}
            </button>
        {:else}
            <button
                class="danger-text"
                onclick={() => { onCancel(false); }}
                disabled={isCancelling || isResolvingConflict}
            >
                {tString('fileOperations.transferProgress.conflictCancel')}
            </button>
        {/if}
    </div>
</div>

{#snippet strong(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}

<style>
    /* Conflict section */
    .conflict-section {
        padding: var(--spacing-md) var(--spacing-xl) var(--spacing-xl);
    }

    .conflict-filename {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .conflict-comparison {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    /* Red warning block for file→folder clashes. Sits below the title and
       above the filename so the user sees the destructive nature before any
       button. Mirrors the warning-callout visual vocabulary used elsewhere
       (icon + sentence in a tinted block) but in red, not yellow, to mark
       the higher destructive stakes. */
    .conflict-warning {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        border: 1px solid var(--color-error-border);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .conflict-warning strong {
        font-weight: 600;
    }

    .conflict-warning-icon {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        color: var(--color-error-text);
        margin-top: 1px;
    }

    /* `(unknown)` placeholder used in the Existing-size slot when the BE
       couldn't look up the destination folder's size (no drive-index entry).
       Muted so it reads as "no value" rather than masquerading as a real
       byte-range color. */
    .conflict-file-size.unknown {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Wrap so the tooltip has a host element when the inner Button is
       disabled (disabled buttons don't fire pointer events themselves).
       The inner button still gets `flex: 1` via the existing
       `.conflict-buttons :global(button)` rule below, so this wrap matches
       button-row width. */
    .conflict-button-wrap {
        display: flex;
        flex: 1;
        max-width: 200px;
    }

    .conflict-button-wrap > :global(button) {
        flex: 1;
        max-width: none;
    }

    .conflict-file {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        justify-content: center;
        flex-wrap: wrap;
    }

    .conflict-file-label {
        color: var(--color-text-tertiary);
        min-width: 55px;
        text-align: right;
    }

    .conflict-file-size {
        font-weight: 500;
        min-width: 70px;
    }

    .conflict-file-date {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .conflict-annotation {
        font-size: var(--font-size-sm);
        font-weight: 500;
    }

    .conflict-annotation.newer {
        color: var(--color-accent-text);
    }

    .conflict-annotation.larger {
        color: var(--color-size-mb);
    }

    .conflict-buttons {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
    }

    .conflict-buttons-row {
        display: flex;
        gap: var(--spacing-sm);
        justify-content: center;
    }

    .conflict-buttons :global(button) {
        flex: 1;
        max-width: 200px;
    }

    .conflict-cancel {
        display: flex;
        justify-content: center;
        gap: var(--spacing-md);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border-strong);
    }

    /* Host for the disabled Rollback button so the tooltip still fires (a
       disabled button swallows its own pointer events). Mirrors
       `.conflict-button-wrap`'s purpose for the smaller-disabled bulk action. */
    .disabled-button-wrap {
        display: inline-flex;
    }

    /* Text-only danger button (for less prominent cancel) */
    .danger-text {
        background: transparent;
        color: var(--color-error-text);
        border: none;
        font-size: var(--font-size-sm);
        font-weight: 500;
        padding: var(--spacing-sm) var(--spacing-lg);
        transition: all var(--transition-base);
    }

    .danger-text:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .danger-text:hover:not(:disabled) {
        text-decoration: underline;
    }
</style>
