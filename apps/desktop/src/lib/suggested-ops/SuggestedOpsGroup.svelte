<script lang="ts">
    // One suggested group, top to bottom in the order a decision is made: what it is (the action
    // and the title, the count, and the pills that change the answer), the facts Cmdr holds by
    // itself (where the files are and where they'd go), the AI's reason (labelled as its words),
    // the files, and, under them, whether it can be taken back beside the button that names the
    // action. A server card is the same order with no files: its facts are what the click saves.
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { SuggestedGroupView } from '#lib/tauri-commands/index.js'
    import {
        approvableCount,
        approveGroup,
        canApprove,
        rejectGroup,
        reloadGroup,
        reviewOf,
        suggestedOpsState,
    } from './suggested-ops-trigger.svelte'
    import { refusalReason } from './suggested-ops-refusal'
    import {
        actionKey,
        approveKey,
        reversibilityKey,
        runsAsKeys,
        shownReversibility,
        shownVerb,
    } from './suggested-ops-copy'
    import AgentReason from './AgentReason.svelte'
    import SizedLabel from './SizedLabel.svelte'
    import SuggestedOpFiles from './SuggestedOpFiles.svelte'
    import SuggestedServerFacts from './SuggestedServerFacts.svelte'
    import VolumePill from './VolumePill.svelte'

    interface Props {
        group: SuggestedGroupView
        /** Every group's action-button label right now, so each button is as wide as the widest
         *  and the buttons line up down the dialog. */
        actionLabels: string[]
    }

    const { group, actionLabels }: Props = $props()

    const review = $derived(reviewOf(group.groupId))
    const count = $derived(approvableCount(group.groupId))
    /** False while the group changed under the review: the user reads the new version first. */
    const approvable = $derived(canApprove(group.groupId))
    const busy = $derived(suggestedOpsState.busyGroupId !== null)
    /** The reason under a group whose last approval didn't start, or `null`. */
    const refusal = $derived.by(() => {
        const answer = suggestedOpsState.refusals.get(group.groupId)
        return answer ? refusalReason(answer, group.verb, group.server) : null
    })
    /** What would really run: a trash on a drive with no Trash is a delete everywhere it's named. */
    const verb = $derived(shownVerb(group.verb, group.runsAs))
    const reversible = $derived(shownReversibility(group.reversible, group.runsAs))
    const irreversible = $derived(reversible === 'irreversible')
    const runsAs = $derived(group.runsAs === null ? null : runsAsKeys(group.runsAs))
    const titleId = $derived(`suggested-group-${String(group.groupId)}-title`)
    const actionLabel = $derived(approveLabel(group, count))

    function approveLabel(of: SuggestedGroupView, approvable: number): string {
        return tString(approveKey(shownVerb(of.verb, of.runsAs)), {
            countText: formatInteger(approvable),
            count: approvable,
        })
    }
</script>

<SectionCard>
    <article class="group" aria-labelledby={titleId}>
        <header class="group-head">
            <!-- The action leads the title, so what happens reads from the title alone. -->
            <h3 class="group-title" id={titleId}>
                <span class="action" class:danger={irreversible}>{tString(actionKey(verb))}</span>
                <span class="title-text">{group.displayName}</span>
            </h3>
            {#if !group.server}
                <span class="count">
                    {tString('suggestedOps.fileCount', {
                        countText: formatInteger(group.totalOpCount),
                        count: group.totalOpCount,
                    })}
                </span>
            {/if}
            {#if runsAs || group.fromSelector}
                <ul class="pills">
                    {#if runsAs}
                        <li class="pill" use:tooltip={tString(runsAs.tooltip)}>{tString(runsAs.pill)}</li>
                    {/if}
                    {#if group.fromSelector}
                        <li class="pill">{tString('suggestedOps.fromPattern')}</li>
                    {/if}
                </ul>
            {/if}
        </header>

        <!-- Facts Cmdr holds by itself, unlabelled as anyone's words: they're the check the AI's
             reason below can be read against. -->
        <dl class="facts selectable">
            {#if group.server}
                <SuggestedServerFacts server={group.server} />
            {:else}
                <dt>{tString(group.verb === 'rename' ? 'suggestedOps.facts.folder' : 'suggestedOps.facts.from')}</dt>
                <dd>
                    <VolumePill volumeId={group.sourceVolumeId} name={group.sourceVolumeName} />
                    <span class="path">{group.sourceFolder ?? tString('suggestedOps.facts.severalFolders')}</span>
                </dd>
                {#if verb === 'trash'}
                    <dt>{tString('suggestedOps.facts.to')}</dt>
                    <dd>{tString('suggestedOps.facts.trash')}</dd>
                {:else if group.destination !== null && verb !== 'rename' && verb !== 'delete'}
                    <dt>{tString(group.verb === 'compress' ? 'suggestedOps.facts.archive' : 'suggestedOps.facts.to')}</dt>
                    <dd>
                        <VolumePill volumeId={group.destinationVolumeId} name={group.destinationVolumeName} />
                        <span class="path">{group.destination}</span>
                        <!-- About the folder itself, so it sits beside it. -->
                        {#if group.destinationState === 'willBeCreated'}
                            <span class="dest-note" use:tooltip={tString('suggestedOps.folderWillBeCreatedTooltip')}>
                                <Icon name="folder-plus" size={14} aria-hidden="true" />
                                {tString('suggestedOps.folderWillBeCreated')}
                            </span>
                        {:else if group.destinationState === 'notConnected'}
                            <span
                                class="dest-note"
                                use:tooltip={tString('suggestedOps.destinationNotConnectedTooltip')}
                            >
                                <Icon name="unplug" size={14} aria-hidden="true" />
                                {tString('suggestedOps.destinationNotConnected')}
                            </span>
                        {:else if group.destinationState === 'unknown'}
                            <span class="dest-note">
                                <Icon name="circle-alert" size={14} aria-hidden="true" />
                                {tString('suggestedOps.destinationUnknown')}
                            </span>
                        {/if}
                    </dd>
                {/if}
            {/if}
        </dl>

        {#if group.server && group.server.sharesAffected !== null}
            <!-- An SMB edit applies to the host, so it reaches every share saved under it. -->
            <p class="shares">
                {tString('suggestedOps.server.sharesAffected', {
                    countText: formatInteger(group.server.sharesAffected),
                    count: group.server.sharesAffected,
                })}
            </p>
        {/if}

        {#if group.rationale}
            <AgentReason text={group.rationale} />
        {/if}

        {#if review?.changedUnderReview}
            <!-- The rows stay exactly where they are. Re-ordering a list somebody is halfway
                 through deciding on is how a wrong row gets approved. -->
            <div class="changed" role="status">
                <span>{tString('suggestedOps.changedUnderReview')}</span>
                <Button size="mini" onclick={() => void reloadGroup(group.groupId)}>
                    {tString('suggestedOps.showNewVersion')}
                </Button>
            </div>
        {/if}

        {#if group.server}
            <!-- No files, no selection, no window: the card above is the whole of what runs. -->
            <div class="footer">
                {@render undoNote()}
                {@render actions()}
            </div>
        {:else}
            <SuggestedOpFiles {group} {undoNote} {actions} />
        {/if}

        {#if refusal}
            <!-- An approval that didn't start gave the group back: it's pending again and nothing
                 ran, so the reason sits right under the button the user just pressed. -->
            <div class="refusal" role="alert">
                <span class="refusal-label"
                    >{tString(group.server ? 'suggestedOps.refusal.serverLabel' : 'suggestedOps.refusal.label')}</span
                >
                <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `refusalReason`: the write-error pipeline escapes every name and path. Same boundary as the queue's failed row. -->
                <p class="refusal-message selectable">{@html refusal.message}</p>
                {#if refusal.suggestion}
                    <p class="refusal-suggestion selectable">{refusal.suggestion}</p>
                {/if}
            </div>
        {/if}
    </article>
</SectionCard>

<!-- Whether the action can be taken back, and why, beside the button that takes it: the last
     thing read before the click. -->
{#snippet undoNote()}
    <p class="undo-note" class:danger={irreversible}>
        <span class="undo-icon" aria-hidden="true">
            <Icon name={irreversible ? 'triangle-alert' : 'rotate-ccw'} size={14} />
        </span>
        <span>{tString(reversibilityKey(verb, reversible), { count: group.totalOpCount })}</span>
    </p>
{/snippet}

{#snippet actions()}
    <!-- eslint-disable-next-line cmdr/dialog-buttons-in-footer -- each suggestion's own Reject / approve pair, one per group in the list; the dialog has no footer actions of its own -->
    <div class="actions">
        <Button disabled={busy} onclick={() => void rejectGroup(group.groupId)}>
            {tString('suggestedOps.reject')}
        </Button>
        <Button
            variant={irreversible ? 'danger' : 'primary'}
            disabled={busy || !approvable}
            onclick={() => void approveGroup(group.groupId)}
        >
            <SizedLabel label={actionLabel} labels={actionLabels} />
        </Button>
    </div>
{/snippet}

<style>
    .group {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .group-head {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        column-gap: var(--spacing-sm);
        row-gap: var(--spacing-xs);
    }

    .group-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
    }

    /* The action reads first and stands apart from the title the AI wrote. */
    .action {
        margin-right: var(--spacing-xs);
        color: var(--color-accent-text);
    }

    /* An action nothing takes back reads in the same tone as its note and its button. */
    .action.danger {
        color: var(--color-error-text);
    }

    .count {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .pills {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-xs);
        margin: 0;
        padding-inline-start: 0;
        list-style: none;
    }

    /* Global inside the group, so a server card's "A new address" tag in its facts is the same
       pill. The card is `--color-bg-secondary`, so a pill steps back to the recessed tone. */
    .group :global(.pill) {
        padding: var(--spacing-xxs) var(--spacing-sm);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-full);
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-tight);
        /* A long reason wraps inside its pill rather than pushing past the dialog edge. */
        max-width: 100%;
        overflow-wrap: break-word;
    }

    .facts {
        display: grid;
        grid-template-columns: max-content minmax(0, 1fr);
        column-gap: var(--spacing-sm);
        row-gap: var(--spacing-xs);
        align-items: center;
        margin: 0;
        /* The global list inset would push the facts off the title's edge. */
        padding-inline-start: 0;
        font-size: var(--font-size-sm);
    }

    /* Global inside the list, so a server card's rows (`SuggestedServerFacts`) read the same. */
    .facts :global(dt) {
        color: var(--color-text-tertiary);
    }

    .facts :global(dd) {
        margin: 0;
        color: var(--color-text-primary);
    }

    .dest-note {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        margin-left: var(--spacing-sm);
        color: var(--color-text-secondary);
        vertical-align: middle;
    }

    .shares {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .path {
        overflow-wrap: anywhere;
    }

    .undo-note {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* "Can't be undone" has to look different from a note that says it can: it's the one that
       decides whether the user approves the group. */
    .undo-note.danger {
        color: var(--color-error-text);
    }

    /* Centers the glyph on the first text line, however the note wraps. */
    .undo-icon {
        display: flex;
        align-items: center;
        height: 1lh;
    }

    .actions {
        display: flex;
        flex-shrink: 0;
        margin-left: auto;
        gap: var(--spacing-sm);
    }

    .changed {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: var(--color-warning-bg);
        color: var(--color-warning-text);
        font-size: var(--font-size-sm);
    }

    .refusal {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        font-size: var(--font-size-sm);
    }

    .refusal-label {
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .refusal-message,
    .refusal-suggestion {
        margin: 0;
    }

    .refusal-message {
        color: var(--color-error-text);
    }

    .refusal-suggestion {
        color: var(--color-text-secondary);
    }
</style>
