<script lang="ts">
    /**
     * The rollback question a tool asked for, on the main window, with nothing under it.
     *
     * Mounted unconditionally by the main page and gated on `tool-rollback.svelte.ts`, like
     * `OperationConflictDialog`. A question about a running operation comes down when that
     * operation can no longer roll back, the way `QueueRow` withdraws its own. One about a finished
     * operation stays up: its yes runs the journal's gate, and a refusal gets said.
     */
    import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'
    import { bindOperationSession } from './operation-session/bind-operation-session.svelte'
    import type { RollbackConfirmResult } from './rollback-confirmers'
    import {
        confirmLoggedToolRollback,
        dismissToolRollback,
        getToolRollbackQuestion,
        liveRow,
        liveRowBlock,
    } from './tool-rollback.svelte'

    const question = $derived(getToolRollbackQuestion())
    const liveId = $derived(question?.target === 'live' ? question.operationId : null)
    const session = bindOperationSession(() => liveId)

    $effect(() => {
        if (liveId !== null && liveRowBlock(liveRow(liveId)) !== null) dismissToolRollback()
    })

    function confirm(): RollbackConfirmResult {
        const asked = question
        if (asked === null) return undefined
        if (asked.target === 'logged') {
            dismissToolRollback()
            return confirmLoggedToolRollback(asked.operationId, asked.initiator)
        }
        // Sent before the question comes down, since that lets go of the session.
        const stopping = session.current?.rollback() ?? Promise.resolve(false)
        dismissToolRollback()
        return stopping.then((sent) =>
            sent
                ? { status: 'dispatched', inverseOpId: null }
                : { status: 'refused', refusal: { kind: 'alreadyStopping' } },
        )
    }
</script>

{#if question}
    <RollbackConfirmDialog
        variant={question.variant}
        finishing={question.finishing}
        onConfirm={confirm}
        onCancel={dismissToolRollback}
    />
{/if}
