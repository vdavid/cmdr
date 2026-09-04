<script lang="ts">
    /**
     * Pause / Resume and Cancel for the reversal a history row is running.
     *
     * A rollback started from the history dialog can work a slow mount for a long
     * time, and the dialog that started it is where a person looks to stop it. The
     * engine has always supported both (it polls its stop flag and parks on its
     * pause gate per item); these are the buttons.
     *
     * ## It commands the REVERSAL, never the row's own operation
     *
     * The row names a finished operation ("Copied 3 items"); the thing that's live
     * is the inverse the journal opened against it, and that's what binds here.
     * The id comes from the row's `inverseOpId`, which the journal fills on every
     * read, so a reversal started in another window, by an agent, or before this
     * dialog opened gets the same buttons as one started under the user's cursor.
     *
     * ## Same commands as every other surface
     *
     * Through `bindOperationSession`, so this is the queue row's Pause and Cancel
     * against the same guards, not a second path: a press here is a press the
     * queue window sees, and one there disables the button here. ❌ Never call the
     * pause / resume / cancel IPC directly from a view.
     *
     * ## What tells them the reversal is over
     *
     * A stale `rolling_back` row (the dialog reads the journal once, on open) must
     * never offer a press with nothing left to press, so the controls follow the
     * live operation, not the row. A transfer would end with a terminal event; the
     * reversal engine emits progress and then nothing at all, and the only word its
     * end gets is dropping out of the registry. That's `session.leftRegistry`, and
     * ❌ dropping it from the liveness test leaves the buttons lit over a finished
     * reversal — Cancel disabling itself forever, Pause staying live.
     *
     * ## The words are the queue's, on purpose
     *
     * `queue.row.*` for all four ("Pause", "Resume", "Cancel", and the "Paused"
     * status word): these are the same controls on the same operation, so ❌ don't
     * mint `operationLog.*` twins — a copy edit would then move one surface and
     * not the other. The accessible name is the visible word plus the
     * `aria-describedby` naming the row, the way the row's Roll back button
     * already works.
     */
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { bindOperationSession } from '$lib/file-operations/operation-session/bind-operation-session.svelte'

    interface Props {
        /** The reversal to command: the operation the journal opened to undo this
         *  row, never the row's own operation. */
        inverseOpId: string
        /** The elements naming the row these buttons belong to, for a screen
         *  reader. Same shape the row's Roll back button uses, so a press is never
         *  announced as a bare "Pause". */
        describedBy: string
    }

    const { inverseOpId, describedBy }: Props = $props()

    const session = bindOperationSession(() => inverseOpId)

    /** The session once the binding takes hold. `null` for the frame between mount
     *  and the first effect, and for a reversal the registry has never heard of. */
    const op = $derived(session.current)

    /** The bar-is-moving truth is the SNAPSHOT status, the same one the queue row
     *  reads: a parked reversal emits no further ticks, so its last one describes
     *  work that has stopped. `null` until the first snapshot lands. */
    const status = $derived(op?.status ?? null)

    const isRunning = $derived(status === 'running')
    const isPaused = $derived(status === 'paused')
    /** Queued behind another operation on the same drive: nothing to park yet, but
     *  cancelling drops it before it ever spawns. */
    const isQueued = $derived(status === 'queued')

    /** A reversal that has ended (or that this window never saw) leaves nothing to
     *  command.
     *
     *  Three readings, because a reversal ends more quietly than a transfer does.
     *  `settled` catches the terminal events; the status catches a snapshot that
     *  says `done`. Neither ever lands for the operation-log reversal: it emits
     *  progress and no terminal event, and its last word is dropping out of the
     *  registry — which is `leftRegistry`. Without that third reading the buttons
     *  stay lit over a finished reversal and each press reaches nothing. */
    const isLive = $derived(
        op !== null && !op.settled && !op.leftRegistry && (isRunning || isPaused || isQueued),
    )
</script>

{#if isLive && op !== null}
    <!-- One flex item for the whole set, and one that never shrinks. The row it
         sits in gives its head `flex: 1 1 auto`, so without this the buttons are
         the ones squeezed — at the dialog's fixed 620 px down to 73, narrower
         than "Resume" and every longer translation of it. -->
    <div class="rollback-controls">
        {#if isPaused}
            <!-- ❌ A paused reversal must not read as a finished one. The row's own badge
                 still says "Rolling back" (journal truth: it is), so this is the word
                 that says it isn't moving — the same status word the queue row shows for
                 the same operation, off the same lifecycle status. -->
            <span class="paused-badge">{tString('queue.row.status', { status: 'paused' })}</span>
        {/if}
        {#if isRunning || isPaused}
            <!-- One button, two words, steered by the snapshot the session already
                 holds, never by a round trip. Pause parks at the next item boundary. -->
            <Button
                size="mini"
                variant="secondary"
                disabled={op.pauseInFlight}
                aria-describedby={describedBy}
                onclick={() => void op.togglePause()}
            >
                <span class="btn-inner">
                    <Icon name={isPaused ? 'play' : 'pause'} size={13} />
                    {isPaused ? tString('queue.row.resume') : tString('queue.row.pause')}
                </span>
            </Button>
        {/if}
        <!-- Cancel keeps what has already come back. What it leaves behind is what the
             journal then calls "partly rolled back", which the row already words and
             already offers to finish — ❌ no second sentence here saying it differently. -->
        <Button
            size="mini"
            variant="secondary"
            disabled={op.cancelling}
            aria-describedby={describedBy}
            onclick={() => void op.cancel()}
        >
            <span class="btn-inner">
                <Icon name="x" size={13} />
                {tString('queue.row.cancel')}
            </span>
        </Button>
    </div>
{/if}

<style>
    /* The whole set is ONE flex item of `.op-row`, and it keeps its natural width:
       the row's head is the `flex: 1 1 auto` part and is what gives way. Spaced
       like the row spaces its own children, so wrapping them changes no gap. */
    .rollback-controls {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        flex-shrink: 0;
    }

    /* Matches the row's own badges (`OperationLogDialog.svelte`), which are scoped
       to that component and can't reach in here. */
    .paused-badge {
        font-size: var(--font-size-xs);
        padding: 1px var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        white-space: nowrap;
        flex-shrink: 0;
    }

    /* Icon + label inside one mini button, like the queue window's row controls.
       `nowrap` because a word broken across two lines ("Resu/me") is never the
       right answer to a narrow row: the label keeps its width and the wrapper
       above refuses to give it up. */
    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        white-space: nowrap;
    }
</style>
