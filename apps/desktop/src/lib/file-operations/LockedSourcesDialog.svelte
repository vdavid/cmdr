<script lang="ts">
    /**
     * The question a move asks, before any byte moves, when some of what it moves
     * is Locked: "3 items you're moving are locked", with Move anyway, Skip locked
     * items, and Cancel.
     *
     * Presentational: `locked-sources.svelte.ts` owns the question and the answers,
     * and `LockedSourcesPrompt.svelte` binds the two. No `onclose`, so no × and no
     * Escape: every way out decides something about the person's files, and a
     * reflex Escape on a prompt that appeared on its own shouldn't pick one.
     */
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { OperationSnapshot, WriteLockedSourcesEvent } from '#lib/tauri-commands/index.js'
    import { getFolderName } from './transfer/transfer-dialog-utils'

    interface Props {
        /** The backend's question. */
        event: WriteLockedSourcesEvent
        /** The operation's queue row, for naming which move is asking. */
        snapshot: OperationSnapshot | null
        /** An answer is in flight: every button disables. */
        answering: boolean
        /** Unlock them for the move, and lock them again where they land. */
        onMoveAnyway: () => void
        /** Leave the Locked items where they are, and move the rest. */
        onSkip: () => void
        /** End the move. */
        onCancel: () => void
    }

    const { event, snapshot, answering, onMoveAnyway, onSkip, onCancel }: Props = $props()

    /** Which move is asking, in the conflict prompt's own words: a backgrounded
     *  one can ask while others run, and the buttons act on exactly one. */
    const context = $derived(
        snapshot === null
            ? null
            : tString('fileOperations.operationConflict.context', {
                  type: snapshot.operationType,
                  hasDestination: snapshot.destination === null ? 'no' : 'yes',
                  destination: snapshot.destination === null ? '' : getFolderName(snapshot.destination),
              }),
    )

    const message = $derived(
        tString('fileOperations.lockedSources.message', {
            count: event.lockedCount,
            countText: formatInteger(event.lockedCount),
        }),
    )

    /** The Locked items the backend didn't name: the count is the whole truth,
     *  the names only help the person recognize them. */
    const unnamed = $derived(Math.max(0, event.lockedCount - event.lockedSample.length))
</script>

<ModalDialog
    dialogId="locked-sources"
    titleId="locked-sources-title"
    ariaDescribedby="locked-sources-message"
    containerStyle="width: 480px; min-width: 380px"
    resizable="horizontal"
    opensOnItsOwn
>
    {#snippet title()}{tString('fileOperations.lockedSources.title')}{/snippet}

    {#if context !== null}
        <p class="context">{context}</p>
    {/if}

    <p id="locked-sources-message" class="message">{message}</p>

    <ul class="locked-list">
        {#each event.lockedSample as path (path)}
            <li class="locked-name" use:tooltip={{ text: path, overflowOnly: true }}>{getFolderName(path)}</li>
        {/each}
        {#if unnamed > 0}
            <li class="locked-more">
                {tString('fileOperations.lockedSources.moreItems', {
                    count: unnamed,
                    countText: formatInteger(unnamed),
                })}
            </li>
        {/if}
    </ul>

    {#snippet footer()}
        <Button variant="secondary" disabled={answering} onclick={onCancel}
            >{tString('fileOperations.button.cancel')}</Button
        >
        <Button variant="secondary" disabled={answering} onclick={onSkip}
            >{tString('fileOperations.lockedSources.skip')}</Button
        >
        <!-- Moving what the person asked to move is the answer meant nearly every
             time, and each item ends up Locked again where it lands, so it's the
             one Enter presses. -->
        <!-- Never autofocused: this clears a Locked flag, so Enter mustn't press it. -->
        <Button variant="primary" disabled={answering} onclick={onMoveAnyway}
            >{tString('fileOperations.lockedSources.moveAnyway')}</Button
        >
    {/snippet}
</ModalDialog>

<style>
    .context {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
    }

    .locked-list {
        margin: 0;
        padding: 0;
        list-style: none;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .locked-name {
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .locked-more {
        color: var(--color-text-tertiary);
    }
</style>
