<!--
  What the user answered when the agent suggested something, in the thread that suggested it.

  Two rows in the transcript render through here, deliberately as one block: a timeline event
  carrying a single decision as it happens (an approval's says "running" until its operation
  settles, then the rail patches it with the outcome), and the opener of the follow-up turn a rejected
  sweep earns, carrying everything the user turned down at once.

  ⚠️ The backend hands this over as VERBS, COUNTS, and the group's own display text, never as
  the sentence the model read (`agent/types.rs`, `ProposalDecision`). Every word here is ours
  and is translated. So: never render a backend string in this block, and render the path as
  escaped plain text (a folder name is attacker-controlled), never {@html}.
-->
<script lang="ts">
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { ProposalDecision } from '#lib/tauri-commands/index.js'

    interface Props {
        decisions: ProposalDecision[]
    }
    const { decisions }: Props = $props()

    /** Every verb spelled out, so each key has a literal call site: a runtime-built key would
     *  read as dead to `desktop-message-keys-unused` and get translated for nothing. */
    const VERB_KEYS: Record<ProposalDecision['verb'], MessageKey> = {
        move: 'askCmdr.decision.verbMove',
        copy: 'askCmdr.decision.verbCopy',
        trash: 'askCmdr.decision.verbTrash',
        delete: 'askCmdr.decision.verbDelete',
        rename: 'askCmdr.decision.verbRename',
        compress: 'askCmdr.decision.verbCompress',
        extract: 'askCmdr.decision.verbExtract',
        addServer: 'askCmdr.decision.verbAddServer',
        editServer: 'askCmdr.decision.verbEditServer',
    }

    /** The headline: what was asked for, over how much, and what the person said. A server
     *  suggestion has no files, so its headline names no count. `verb` goes along with the
     *  translated name so a branch can own a whole sentence where a slot can't fit: Windows says
     *  "moving 3 items to the Recycle Bin", the place after the count. */
    function headline(decision: ProposalDecision): string {
        const rejected = decision.outcome.kind === 'rejected'
        if (decision.verb === 'addServer' || decision.verb === 'editServer') {
            const serverKey = rejected ? 'askCmdr.decision.serverRejected' : 'askCmdr.decision.serverApproved'
            return tString(serverKey, { verbName: tString(VERB_KEYS[decision.verb]) })
        }
        const key = rejected ? 'askCmdr.decision.rejected' : 'askCmdr.decision.approved'
        return tString(key, {
            verb: decision.verb,
            verbName: tString(VERB_KEYS[decision.verb]),
            countText: formatInteger(decision.ops),
            count: decision.ops,
        })
    }

    /** Where the run is, or what it actually did, which is not always what was approved.
     *  `null` for a rejection, which never ran at all. */
    function result(decision: ProposalDecision): string | null {
        const outcome = decision.outcome
        switch (outcome.kind) {
            case 'rejected':
                return null
            case 'running':
                return tString('askCmdr.decision.running')
            case 'interrupted':
                return tString('askCmdr.decision.interrupted')
            case 'ran':
                return tString('askCmdr.decision.result', {
                    doneText: formatInteger(outcome.done),
                    skippedText: formatInteger(outcome.skipped),
                    failedText: formatInteger(outcome.failed),
                })
            case 'saved':
                return tString(outcome.note === 'shareNotSaved' ? 'askCmdr.decision.savedNotShare' : 'askCmdr.decision.saved')
        }
    }
</script>

<ul class="decisions">
    {#each decisions as decision, index (index)}
        <li>
            <span class="glyph"><Icon name="bot" size={13} aria-hidden="true" /></span>
            <div class="lines">
                <span>{headline(decision)}</span>
                <span class="what" use:tooltip={{ text: decision.what, overflowOnly: true }}>{decision.what}</span>
                {#if result(decision)}
                    <span class="result">{result(decision)}</span>
                {/if}
            </div>
        </li>
    {/each}
</ul>

<style>
    .decisions {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin: 0;
        padding: var(--spacing-xs);
        list-style: none;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .decisions li {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .glyph {
        display: flex;
        width: 16px;
        justify-content: center;
        flex: none;
        padding-top: 1px;
        color: var(--color-text-tertiary);
    }

    .lines {
        display: flex;
        flex-direction: column;
        min-width: 0;
    }

    .what {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
    }

    .result {
        color: var(--color-text-tertiary);
    }
</style>
