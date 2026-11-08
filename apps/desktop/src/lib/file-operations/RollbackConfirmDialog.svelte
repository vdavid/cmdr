<script lang="ts">
    /**
     * The one question in front of Rollback.
     *
     * ⚠️ **The body has to match the operation.** Undoing a copy deletes what the copy
     * wrote, and a destination it OVERWROTE is one of those: no backup of the replaced
     * original is kept (`crates/cmdr-operations/src/transfer/CLAUDE.md` § "Overwrite isn't
     * reversible"). Undoing a move deletes NOTHING; the files travel home. Wording every
     * rollback as a delete would scare people off an operation that takes nothing away,
     * and wording every one as a restore would hide a real deletion. Hence `variant`.
     * Rationale and the surfaces that raise it: `DETAILS.md` § "Rollback asks first".
     *
     * The safe answer takes focus in every variant, so a reflex Enter never rolls back.
     * Presentational: each host owns the pending-rollback state and calls its own
     * rollback in `onConfirm`. It registers that yes for `dialog_confirm`
     * (`rollback-confirmers.ts`), whichever host raised it.
     */
    import { onMount } from 'svelte'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import type { ToolInitiator } from '#lib/ui/started-elsewhere-guard.js'
    import Button from '#lib/ui/Button.svelte'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { RollbackConfirmVariant } from './reversal-wording'
    import { registerRollbackConfirmer, type RollbackConfirmResult } from './rollback-confirmers'

    interface Props {
        variant: RollbackConfirmVariant
        /**
         * The operation is already PARTLY rolled back, so this press rolls back the
         * rest instead of starting a fresh reversal. It swaps the title and the
         * confirming button; the body stays whatever `variant` says, because what
         * happens to the files is the same either way.
         */
        rest?: boolean
        /**
         * The press takes only SOME of the operation's items (the Operation log's tree
         * left some unchecked). The title, the button, and the body then speak of "the
         * selected" items, whether or not `rest` is set. Only an undo variant
         * has a narrowed body; a running operation's stop is never a subset.
         */
        subset?: boolean
        /** Go ahead: reverse the operation. A host undoing a finished operation returns what
         *  came of it; one stopping a running operation returns nothing. */
        onConfirm: () => RollbackConfirmResult
        /** Leave the operation exactly as it is. Also what Escape and × do. */
        onCancel: () => void
        /** Who to name when a tool asked this, under the arming guard (`ModalDialog`). */
        startedBy?: ToolInitiator
    }

    const { variant, rest = false, subset = false, onConfirm, onCancel, startedBy }: Props = $props()

    onMount(() => registerRollbackConfirmer(() => onConfirm()))

    /**
     * Body + cancel wording + whether the confirming button reads as destructive.
     * Only the two DELETING variants get `danger`: red on "put my files back" would
     * cry wolf, and this app spends that colour on operations that take something
     * away. That's why a running move takes `stopAndMoveBack` rather than sharing
     * the running copy's arm — its reversal removes nothing.
     */
    function wording(
        v: RollbackConfirmVariant,
        selected: boolean,
    ): { body: MessageKey; cancel: MessageKey; destructive: boolean } {
        switch (v) {
            case 'stopAndDelete':
                return {
                    body: 'fileOperations.rollbackConfirm.body',
                    cancel: 'fileOperations.rollbackConfirm.keep',
                    destructive: true,
                }
            case 'stopAndMoveBack':
                return {
                    body: 'fileOperations.rollbackConfirm.bodyStopAndMoveBack',
                    cancel: 'fileOperations.rollbackConfirm.keep',
                    destructive: false,
                }
            case 'undoByDeleting':
                return {
                    body: selected
                        ? 'fileOperations.rollbackConfirm.bodyUndoByDeletingSelected'
                        : 'fileOperations.rollbackConfirm.bodyUndoByDeleting',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: true,
                }
            case 'undoByMovingBack':
                return {
                    body: selected
                        ? 'fileOperations.rollbackConfirm.bodyUndoByMovingBackSelected'
                        : 'fileOperations.rollbackConfirm.bodyUndoByMovingBack',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: false,
                }
            case 'undoByRenamingBack':
                return {
                    body: selected
                        ? 'fileOperations.rollbackConfirm.bodyUndoByRenamingBackSelected'
                        : 'fileOperations.rollbackConfirm.bodyUndoByRenamingBack',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: false,
                }
            case 'undoByRestoringTags':
                return {
                    body: selected
                        ? 'fileOperations.rollbackConfirm.bodyUndoByRestoringTagsSelected'
                        : 'fileOperations.rollbackConfirm.bodyUndoByRestoringTags',
                    cancel: 'fileOperations.rollbackConfirm.leaveAsIs',
                    destructive: false,
                }
        }
    }

    const words = $derived(wording(variant, subset))

    /** Title and confirming button: a subset first (it's what the press takes), then the rest of a partial one. */
    const titleKey: MessageKey = $derived(
        subset
            ? 'fileOperations.rollbackConfirm.titleSelected'
            : rest
              ? 'fileOperations.rollbackConfirm.titleRest'
              : 'fileOperations.rollbackConfirm.title',
    )
    const confirmLabel: MessageKey = $derived(
        subset
            ? 'fileOperations.rollbackConfirm.rollBackSelected'
            : rest
              ? 'fileOperations.rollbackConfirm.rollBackRest'
              : 'fileOperations.rollbackConfirm.rollBack',
    )
</script>

<ModalDialog
    dialogId="rollback-confirmation"
    titleId="rollback-confirmation-title"
    ariaDescribedby="rollback-confirmation-body"
    {startedBy}
    onclose={onCancel}
    containerStyle="max-width: 440px"
>
    {#snippet title()}{tString(titleKey)}{/snippet}

    <p id="rollback-confirmation-body" class="rollback-confirm-body">
        {tString(words.body)}
    </p>

    {#snippet footer()}
        <!-- The safe answer takes focus, so a reflex Enter keeps things as they are. -->
        <Button variant="secondary" autoFocus onclick={onCancel}>{tString(words.cancel)}</Button>
        <Button
            variant={words.destructive ? 'danger' : 'primary'}
            onclick={() => {
                void onConfirm()
            }}
            >{tString(confirmLabel)}</Button
        >
    {/snippet}
</ModalDialog>

<style>
    .rollback-confirm-body {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
