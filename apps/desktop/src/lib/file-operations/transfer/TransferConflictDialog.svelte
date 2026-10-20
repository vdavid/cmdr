<script lang="ts">
    import { untrack } from 'svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import { containingFolder } from './transfer-dialog-utils'
    import { formatByteSize, dynamicTierIndex } from '#lib/units/index.js'
    import { getFileSizeFormat } from '#lib/settings/reactive-settings.svelte.js'
    import { sizeTierClasses } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { formatDate } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { LOWER_BOUND_GLYPH } from '#lib/file-explorer/views/full-list-utils.js'
    import type { WriteConflictEvent } from '#lib/tauri-commands/index.js'
    import type { ConflictResolution } from '#lib/file-explorer/types.js'
    import type { KnownSize, RollbackSupport } from '#lib/ipc/bindings.js'
    import { answerDecisionKey, CONFLICT_KEYS, DECISION_KEYS_ARM_MS, type DecisionChoice } from '../decision-keys'
    import { rollbackBlockedTooltipKey } from '../reversal-wording'

    interface Props {
        /** The conflict to resolve (one clash; the BE re-prompts per remaining clash). */
        conflictEvent: WriteConflictEvent
        /** Operation-type flags, driving the bottom Rollback/Cancel row. */
        isCopy: boolean
        isMove: boolean
        /** Whether the backend can reverse this operation, and if not, why
         *  (`null` while unknown, which offers Rollback). When it can't, the
         *  Rollback affordance renders disabled with a tooltip saying why, and a
         *  plain Cancel sits alongside it so there's always a way out. */
        rollbackSupport: RollbackSupport | null
        /** Disables the cancel/rollback buttons while a cancel is in flight. */
        isCancelling: boolean
        /** Disables every resolution button while a resolution IPC is in flight. */
        isResolvingConflict: boolean
        /** Resolve this clash (skip/rename/overwrite/…), optionally applying to all. */
        onResolve: (resolution: ConflictResolution, applyToAll: boolean) => void
        /** Back out of the operation. `rollback` reverses already-written files. */
        onCancel: (rollback: boolean) => void
    }

    const { conflictEvent, isCopy, isMove, rollbackSupport, isCancelling, isResolvingConflict, onResolve, onCancel }: Props =
        $props()

    /** Size-tier class for the "Existing" / "New" size cells. Tiers off the SAME
     *  magnitude ladder the size column uses, so a size and its color agree and
     *  both follow the user's binary/SI base. */
    function getSizeColorClass(bytes: number): string {
        return sizeTierClasses[dynamicTierIndex(bytes, getFileSizeFormat())]
    }

    /** Why Rollback is off, worded per reason (`../reversal-wording.ts`), or
     *  `null` while it works. */
    const rollbackBlockedTooltip = $derived.by(() => {
        const key = rollbackSupport === null ? null : rollbackBlockedTooltipKey(rollbackSupport)
        return key === null ? null : tString(key)
    })

    // The same shape — filename, "Existing:" / "New:" rows, the 4×2 button grid,
    // the Rollback row — serves every clash type. Variants differ only in the row
    // labels, the red warning block above the filename for a file↔folder swap
    // (worded per direction), and the "Overwrite" button copy for file→folder.
    const fileName = $derived(conflictEvent.destinationPath.split('/').pop() ?? '')
    /** Which `f001` this is. A copy of a deep tree raises one prompt per clash,
     *  and a QA pass over 1,600 folders that each held an `f001` got 1,600
     *  identical-looking questions. The folder is what tells them apart, so it
     *  sits under the name, mid-truncated toward its deepest segments (the ones
     *  that differ) with the whole path on hover. */
    const destinationFolder = $derived(containingFolder(conflictEvent.destinationPath))
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
    // Size renders normally when exact, with the listing's `≥` when the drive
    // index has counted only part of a folder, and substitutes `(unknown)` in
    // muted color when the BE could not vouch for a folder's size. ❌ A floor
    // never renders bare: a 40 GB folder counted to 12 KB would read as 12 KB
    // right where an Overwrite deletes it. The color class and formatted text are
    // precomputed here (guarding the null) so the markup's `{:else}` branch needs
    // no cross-variable narrowing.
    const destSize = $derived(conflictEvent.destinationSize)
    const destSizeUnknown = $derived(destSize === null)
    const destSizeIsFloor = $derived(destSize?.kind === 'atLeast')
    const destSizeClass = $derived(sizeClass(destSize))
    const destSizeText = $derived(sizeText(destSize))
    const srcSize = $derived(conflictEvent.sourceSize)
    const srcSizeUnknown = $derived(srcSize === null)
    const srcSizeIsFloor = $derived(srcSize?.kind === 'atLeast')
    const srcSizeClass = $derived(sizeClass(srcSize))
    const srcSizeText = $derived(sizeText(srcSize))
    /** "Overwrite all smaller" compares against the existing side, so it needs
     *  that side's whole size: a floor can't say which is smaller. */
    const destSizeComparable = $derived(destSize?.kind === 'exact')
    const smallerDisabledTooltip = $derived(
        destSizeComparable ? undefined : tString('fileOperations.transferProgress.smallerDisabledTooltip'),
    )
    const lowerBoundTooltip = $derived(tString('fileExplorer.dirSize.lowerBoundLine'))

    function sizeClass(size: KnownSize | null): string {
        return size === null ? '' : getSizeColorClass(size.bytes)
    }

    function sizeText(size: KnownSize | null): string {
        return size === null ? '' : formatByteSize(size.bytes)
    }

    // One function per answer, shared by the button and its letter key.
    const skip = () => { onResolve('skip', false) }
    const skipAll = () => { onResolve('skip', true) }
    const rename = () => { onResolve('rename', false) }
    const renameAll = () => { onResolve('rename', true) }
    const overwrite = () => { onResolve('overwrite', false) }
    const overwriteAll = () => { onResolve('overwrite', true) }
    const overwriteAllSmaller = () => { onResolve('overwrite_smaller', true) }
    const overwriteAllOlder = () => { onResolve('overwrite_older', true) }
    const cancel = () => { onCancel(false) }
    const rollBack = () => { onCancel(true) }

    /** The bottom row's live exit: a plain Cancel, or a Rollback that asks first. */
    const offersRollback = $derived(rollbackBlockedTooltip === null && (isCopy || isMove))
    const exitLocked = $derived(isCancelling || isResolvingConflict)

    /** Every button's letter, enabled exactly when the button is. `decision-keys.ts`. */
    const keyChoices = $derived<DecisionChoice[]>([
        { key: CONFLICT_KEYS.skip, enabled: !isResolvingConflict, run: skip },
        { key: CONFLICT_KEYS.skipAll, enabled: !isResolvingConflict, run: skipAll },
        { key: CONFLICT_KEYS.rename, enabled: !isResolvingConflict, run: rename },
        { key: CONFLICT_KEYS.renameAll, enabled: !isResolvingConflict, run: renameAll },
        { key: CONFLICT_KEYS.overwrite, enabled: !isResolvingConflict, run: overwrite },
        { key: CONFLICT_KEYS.overwriteAll, enabled: !isResolvingConflict, run: overwriteAll },
        {
            key: CONFLICT_KEYS.overwriteAllSmaller,
            enabled: !isResolvingConflict && destSizeComparable,
            run: overwriteAllSmaller,
        },
        { key: CONFLICT_KEYS.overwriteAllOlder, enabled: !isResolvingConflict, run: overwriteAllOlder },
        { key: CONFLICT_KEYS.cancel, enabled: !exitLocked, run: cancel },
        { key: CONFLICT_KEYS.rollback, enabled: offersRollback && !exitLocked, run: rollBack },
    ])

    let section: HTMLDivElement | undefined = $state()
    /** The clash whose keys are live: set `DECISION_KEYS_ARM_MS` after it appears. */
    let armedConflictId = $state<number | null>(null)

    // Per clash: re-arm, and take focus back if it was lost. The progress dialog
    // swaps its whole body for this one, so a focused Pause or Cancel button
    // unmounts and focus falls to <body>, where no dialog hears a key. Focus
    // that's anywhere else (another dialog stacked on top) is left alone.
    $effect(() => {
        const conflictId = conflictEvent.conflictId
        const active = document.activeElement
        // Untracked: the section binding landing must not restart the arming.
        if (active === null || active === document.body) untrack(() => section)?.focus()
        const timer = setTimeout(() => {
            armedConflictId = conflictId
        }, DECISION_KEYS_ARM_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    /**
     * Answers the clash from a bare letter (or Enter = Skip, the safe default,
     * and Escape = Cancel, which keeps what's written). The host dialog forwards
     * its keydowns here, so only a keypress inside THAT dialog can answer. Neither
     * host gives `ModalDialog` an `onclose` while a clash is up, so Escape reaches
     * this body instead of closing anything. Returns whether the key answered.
     */
    export function handleKeydown(event: KeyboardEvent): boolean {
        if (armedConflictId !== conflictEvent.conflictId) return false
        return answerDecisionKey(event, keyChoices, CONFLICT_KEYS.skip)
    }
</script>

<div class="conflict-section" bind:this={section} tabindex="-1">
    {#if isTypeMismatch}
        <!-- Red warning sits below the title and above the filename.
             The "boring" title is `File already exists`; the type swap gets
             called out here, in both directions, so the user can't miss it. -->
        <p
            class="conflict-warning"
            role="alert"
            data-clash={isFileOverFolder ? 'file-over-folder' : 'folder-over-file'}
        >
            <span class="conflict-warning-icon" aria-hidden="true">
                <Icon name="triangle-alert" size={16} />
            </span>
            <span>
                <Trans
                    key={isFileOverFolder
                        ? 'fileOperations.transferProgress.warningFileOverFolder'
                        : 'fileOperations.transferProgress.warningFolderOverFile'}
                    snippets={{ strong }}
                />
            </span>
        </p>
    {/if}

    <!-- Filename, and the folder that says which one it is -->
    <div class="conflict-subject">
        <p class="conflict-filename" use:tooltip={{ text: conflictEvent.destinationPath, overflowOnly: true }}>
            {fileName}
        </p>
        {#if destinationFolder !== null}
            <p
                class="conflict-folder"
                use:useShortenMiddle={{ text: destinationFolder, preferBreakAt: '/', startRatio: 0.3 }}
            ></p>
        {/if}
        {#if conflictEvent.destinationIsLookAlike}
            <!-- The name above is the destination entry's own spelling, the
                 one Overwrite replaces and keeps. The incoming name prints
                 identically, so this line is the only sign the two differ. -->
            <p class="conflict-look-alike">{tString('fileOperations.transferProgress.lookAlikeHint')}</p>
        {/if}
    </div>

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
            {:else if destSizeIsFloor}
                <span class="conflict-file-size {destSizeClass}" use:tooltip={lowerBoundTooltip}
                    ><span class="size-lower-bound-prefix">{LOWER_BOUND_GLYPH}</span>{destSizeText}</span
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
            {:else if srcSizeIsFloor}
                <span class="conflict-file-size {srcSizeClass}" use:tooltip={lowerBoundTooltip}
                    ><span class="size-lower-bound-prefix">{LOWER_BOUND_GLYPH}</span>{srcSizeText}</span
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

    <!-- Buttons. One grid, two columns, every button (the ways out too) as wide
         as the widest label and as tall as the rest (CSS below). The answers:
         left = this-item, right = apply-to-all. Their last row holds the
         conditional bulk variants: `Overwrite all smaller` only works when the
         destination size is known (a folder dest with no index size disables
         it with a tooltip); `Overwrite all older` always stays enabled (mtime
         is always available even for folder destinations). -->
    <div class="conflict-choices">
    <div class="conflict-buttons">
            <Button
                variant="secondary"
                onclick={skip}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.skip}
                aria-keyshortcuts="{CONFLICT_KEYS.skip} Enter"
            >
                {tString('fileOperations.transferProgress.conflictSkip')}
            </Button>
            <Button
                variant="secondary"
                onclick={skipAll}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.skipAll}
                aria-keyshortcuts={CONFLICT_KEYS.skipAll}
            >
                {tString('fileOperations.transferProgress.conflictSkipAll')}
            </Button>
            <Button
                variant="secondary"
                onclick={rename}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.rename}
                aria-keyshortcuts={CONFLICT_KEYS.rename}
            >
                {tString('fileOperations.transferProgress.conflictRename')}
            </Button>
            <Button
                variant="secondary"
                onclick={renameAll}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.renameAll}
                aria-keyshortcuts={CONFLICT_KEYS.renameAll}
            >
                {tString('fileOperations.transferProgress.conflictRenameAll')}
            </Button>
            <Button
                variant="secondary"
                onclick={overwrite}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.overwrite}
                aria-keyshortcuts={CONFLICT_KEYS.overwrite}
            >
                {overwriteLabel}
            </Button>
            <Button
                variant="secondary"
                onclick={overwriteAll}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.overwriteAll}
                aria-keyshortcuts={CONFLICT_KEYS.overwriteAll}
            >
                {overwriteAllLabel}
            </Button>
            <span use:tooltip={smallerDisabledTooltip} class="conflict-button-wrap">
                <Button
                    variant="secondary"
                    onclick={overwriteAllSmaller}
                    disabled={isResolvingConflict || !destSizeComparable}
                    keyHint={CONFLICT_KEYS.overwriteAllSmaller}
                    aria-keyshortcuts={CONFLICT_KEYS.overwriteAllSmaller}
                >
                    {tString('fileOperations.transferProgress.conflictOverwriteAllSmaller')}
                </Button>
            </span>
            <Button
                variant="secondary"
                onclick={overwriteAllOlder}
                disabled={isResolvingConflict}
                keyHint={CONFLICT_KEYS.overwriteAllOlder}
                aria-keyshortcuts={CONFLICT_KEYS.overwriteAllOlder}
            >
                {tString('fileOperations.transferProgress.conflictOverwriteAllOlder')}
            </Button>
    </div>

    <!-- The ways out, under a divider, in the same grid: as big as the answers,
         and red, since one of them undoes work. A plain Cancel (stop, keep what's
         written) is always there, so the user can always back out. A copy or
         move adds Rollback beside it; one the backend can't reverse shows
         Rollback DISABLED, with a tooltip saying why. -->
    <hr class="conflict-divider" />
    <div class="conflict-cancel">
        <Button
            variant="danger"
            onclick={cancel}
            disabled={exitLocked}
            keyHint={CONFLICT_KEYS.cancel}
            aria-keyshortcuts={CONFLICT_KEYS.cancel}
        >
            {tString('fileOperations.transferProgress.conflictCancel')}
        </Button>
        {#if rollbackBlockedTooltip !== null}
            <!-- Blocked, not `disabled`: the reason is worth reading, and a
                 disabled button leaves the tab order with its tooltip. No click
                 handler, so pressing it does nothing. -->
            <Button variant="danger" ariaDisabled tooltipContent={rollbackBlockedTooltip}>
                {tString('fileOperations.transferProgress.conflictRollback')}
            </Button>
        {:else if offersRollback}
            <Button
                variant="danger"
                onclick={rollBack}
                disabled={exitLocked}
                keyHint={CONFLICT_KEYS.rollback}
                aria-keyshortcuts={CONFLICT_KEYS.rollback}
            >
                {tString('fileOperations.transferProgress.conflictRollback')}
            </Button>
        {/if}
    </div>
    </div>
</div>

{#snippet strong(children: import('svelte').Snippet)}<strong>{@render children()}</strong>{/snippet}

<style>
    /* Conflict section. Its width comes from the button grid alone: the host dialog
       sizes to `max-content` (above its own floor), and every text block here takes
       `contain: inline-size`, so a long filename, path, or warning wraps or truncates
       inside whatever width the buttons asked for instead of widening the dialog. */
    .conflict-section {
        padding: var(--spacing-md) var(--spacing-xl) var(--spacing-xl);
    }

    .conflict-subject,
    .conflict-comparison,
    .conflict-warning {
        contain: inline-size;
    }

    /* Focused only to catch the answer keys when the dialog's own focus was
       lost (see the script); a ring around the whole body would read as a
       control. */
    .conflict-section:focus {
        outline: none;
    }

    /* Name and folder read as one subject, so the gap to the comparison rows
       belongs to the pair rather than to whichever line happens to be last. */
    .conflict-subject {
        margin-bottom: var(--spacing-md);
    }

    .conflict-filename {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* The folder under the name: quiet, because the name is what the buttons
       act on and this only says which one. `useShortenMiddle` owns the
       truncation and the hover, so no ellipsis rules here. */
    .conflict-folder {
        margin: var(--spacing-xxs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    /* Explains a look-alike clash. Secondary, not a warning: nothing extra is
       at stake, the prompt only needs to say why a matching name is asked
       about. Narrowed so two short sentences wrap into a readable block. */
    .conflict-look-alike {
        margin: var(--spacing-sm) auto 0;
        max-width: 36em;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        text-align: center;
    }

    .conflict-comparison {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-lg);
        font-size: var(--font-size-sm);
    }

    /* Red warning block for file↔folder clashes. Sits below the title and
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

    /* The listing's `≥` lower-bound prefix (`FullList.svelte`): a qualifier on the
       number, not a digit. */
    .size-lower-bound-prefix {
        color: var(--color-text-secondary);
        margin-right: 1px;
    }

    /* Wrap so the tooltip has a host element when the inner Button is
       disabled (disabled buttons don't fire pointer events themselves). It's
       the grid cell, so its button fills it like the others. */
    .conflict-button-wrap {
        display: flex;
    }

    .conflict-button-wrap > :global(button) {
        flex: 1;
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

    /* Every button as wide as the widest, and never a wrapped label: in a grid
       sized to its content, `1fr` columns each resolve to the widest cell's
       max-content, so both match it and every button fills its cell. A longer
       translation widens the grid, and with it the dialog (the section's comment
       above); a label itself can't wrap (`Button`'s key slot is `nowrap`). The
       answers and the ways out are two groups in markup but cells of this ONE
       grid (`display: contents`), so Cancel and Rollback can't drift from the
       answers' width. */
    .conflict-choices {
        display: grid;
        grid-template-columns: repeat(2, 1fr);
        gap: var(--spacing-sm);
        width: max-content;
        margin: 0 auto;
    }

    .conflict-buttons,
    .conflict-cancel {
        display: contents;
    }

    .conflict-choices :global(button) {
        width: 100%;
    }

    /* Copy and move always pair Cancel with Rollback; anything else gets Cancel
       alone, one column wide and centered under the pair above it. */
    .conflict-cancel > :global(button:only-child) {
        grid-column: 1 / -1;
        justify-self: center;
        width: calc((100% - var(--spacing-sm)) / 2);
    }

    .conflict-divider {
        grid-column: 1 / -1;
        width: 100%;
        margin: var(--spacing-sm) 0;
        border: none;
        border-top: 1px solid var(--color-border-strong);
    }
</style>
