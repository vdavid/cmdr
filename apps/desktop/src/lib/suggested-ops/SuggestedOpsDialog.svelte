<script lang="ts">
    // The AI suggestions dialog: what Ask Cmdr proposed, laid out so the user can decide.
    //
    // The layout is the argument. Each group shows the AI's reason under a label that says
    // whose words they are, next to facts Cmdr holds by itself, so a hallucinated claim is
    // visible AS a claim rather than reading like a finding. Reversibility and a folder that
    // doesn't exist yet are DISCLOSED here and never block: once the user approves, it is
    // exactly as if they started the action.
    //
    // Every group's file list shows right away, each over its own WINDOW the store fetches,
    // because a group of 60,000 ops is legitimate and neither a list nor the wire may grow
    // with it (`SuggestedOpFiles.svelte`).
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import { formatTimeOfDay } from '#lib/settings/format-utils.js'
    import { getSetting, setSetting } from '#lib/settings/index.js'
    import type { SuggestedSweepView } from '#lib/tauri-commands/index.js'
    import { approvableCount, closeSuggestedOps, pendingGroups, suggestedOpsState } from './suggested-ops-trigger.svelte'
    import { approveKey, shownVerb, sweepHeadingHidden, sweepWhen } from './suggested-ops-copy'
    import AgentReason from './AgentReason.svelte'
    import SuggestedOpsGroup from './SuggestedOpsGroup.svelte'

    /** The "nothing runs until you approve it" note, until the user closes it for good. Its
     *  reset is the `suggestedOps.suppressApprovalNotice` switch in Settings > Advanced. */
    let showApprovalNotice = $state(!getSetting('suggestedOps.suppressApprovalNotice'))

    function dismissApprovalNotice(): void {
        showApprovalNotice = false
        setSetting('suggestedOps.suppressApprovalNotice', true)
    }

    /** Every group's action-button label as it reads now, so each button sizes to the widest
     *  (`SizedLabel`) and the buttons line up down the dialog. A server card's takes no count. */
    const actionLabels = $derived(
        pendingGroups().map((group) => {
            const count = approvableCount(group.groupId)
            return tString(approveKey(shownVerb(group.verb, group.runsAs)), { countText: formatInteger(count), count })
        }),
    )

    /** "Now", ticking once a minute so "15 minutes ago" stays true while the dialog is up. */
    let now = $state(Date.now())
    $effect(() => {
        const timer = setInterval(() => (now = Date.now()), 60_000)
        return () => {
            clearInterval(timer)
        }
    })

    /** A sweep heading in words: "Suggested 15 minutes ago", "Suggested yesterday at 14:04". */
    function sweepHeading(sweep: SuggestedSweepView): string {
        const when = sweepWhen(sweep.createdAt, now)
        switch (when.kind) {
            case 'justNow':
                return tString('suggestedOps.sweepWhen.justNow')
            case 'minutesAgo':
                return tString('suggestedOps.sweepWhen.minutesAgo', {
                    countText: formatInteger(when.minutes),
                    count: when.minutes,
                })
            case 'today':
                return tString('suggestedOps.sweepWhen.today', { time: formatTimeOfDay(sweep.createdAt) })
            case 'yesterday':
                return tString('suggestedOps.sweepWhen.yesterday', { time: formatTimeOfDay(sweep.createdAt) })
            case 'earlier':
                return tString('suggestedOps.sweepWhen.earlier', { date: formatDateTime(sweep.createdAt) })
        }
    }
</script>

<ModalDialog
    titleId="suggested-ops-title"
    dialogId="suggested-ops"
    role="dialog"
    onclose={closeSuggestedOps}
    containerStyle="width: 860px; max-width: calc(100vw - 2 * var(--spacing-xl))"
    resizable
>
    {#snippet title()}
        <span>{tString('suggestedOps.title')}</span>
    {/snippet}

    <!-- Only over suggestions to approve: over an empty list or a read that didn't come back it
         would warn about nothing. -->
    {#if showApprovalNotice && suggestedOpsState.sweeps.length > 0}
        <div class="approval-notice" role="note">
            <span class="approval-notice-icon" aria-hidden="true"><Icon name="triangle-alert" size={16} /></span>
            <p>{tString('suggestedOps.description')}</p>
            <button
                type="button"
                class="approval-notice-close"
                aria-label={tString('suggestedOps.approvalNoticeDismiss')}
                use:tooltip={tString('suggestedOps.approvalNoticeDismiss')}
                onclick={dismissApprovalNotice}
            >
                <Icon name="x" size={14} />
            </button>
        </div>
    {/if}

    {#if suggestedOpsState.decisionNotice}
        <!-- Above the list rather than inside a group: after the re-read the group may be gone
             (its approval did land), and the notice still has to be read. -->
        <p class="decision-notice" role="alert">{tString(suggestedOpsState.decisionNotice)}</p>
    {/if}

    <!-- A re-read keeps the groups on screen: only the FIRST read has nothing to show yet.
         Blanking the list on every refresh would throw away where each list was scrolled. -->
    {#if suggestedOpsState.loading && suggestedOpsState.sweeps.length === 0}
        <p class="notice" role="status">{tString('suggestedOps.loadingFiles')}</p>
    {:else if suggestedOpsState.loadError && suggestedOpsState.sweeps.length === 0}
        <p class="notice notice-problem" role="alert">{tString('suggestedOps.loadFailed')}</p>
    {:else if suggestedOpsState.sweeps.length === 0}
        <p class="notice" role="status">{tString('suggestedOps.empty')}</p>
    {:else}
        {#each suggestedOpsState.sweeps as sweep (sweep.sweepId)}
            {@const headingHidden = sweepHeadingHidden(suggestedOpsState.sweeps.length, sweep.createdAt, now)}
            <section class="sweep">
                {#if !headingHidden || sweep.rationale}
                    <header class="sweep-head">
                        {#if !headingHidden}
                            <!-- The heading for the groups under it; the exact moment rides the tooltip. -->
                            <h3 class="sweep-heading">
                                <span use:tooltip={formatDateTime(sweep.createdAt)}>{sweepHeading(sweep)}</span>
                            </h3>
                        {/if}
                        {#if sweep.rationale}
                            <AgentReason text={sweep.rationale} />
                        {/if}
                    </header>
                {/if}

                {#each sweep.groups as group (group.groupId)}
                    <SuggestedOpsGroup {group} {actionLabels} />
                {/each}
            </section>
        {/each}
    {/if}
</ModalDialog>

<style>
    /* The same warning look as the delete dialog's no-Trash banner, plus a way to close it
       for good. Every paragraph here sets its own margin: the global one is a prose rhythm. */
    .approval-notice {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
        background: var(--color-warning-bg);
    }

    .approval-notice p {
        flex: 1 1 auto;
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .approval-notice-icon {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--color-warning);
    }

    .approval-notice-close {
        display: inline-flex;
        flex-shrink: 0;
        padding: 0;
        border: none;
        border-radius: var(--radius-sm);
        background: none;
        color: var(--color-text-secondary);
    }

    .approval-notice-close:hover {
        color: var(--color-text-primary);
    }

    .approval-notice-close:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .notice {
        margin: var(--spacing-lg) 0;
        text-align: center;
        color: var(--color-text-secondary);
    }

    /* A load that didn't come back is the one notice the user has to act on, so it
       carries the error color instead of reading like the empty and loading states. */
    .notice-problem {
        color: var(--color-error-text);
    }

    /* The one notice above the list, so it reads as a banner over what follows, in the tint
       a group's own change notice uses for the same urgency. */
    .decision-notice {
        margin: 0 0 var(--spacing-lg);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: var(--color-error-bg);
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .sweep + .sweep {
        margin-top: var(--spacing-xl);
    }

    .sweep-head {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding-bottom: var(--spacing-md);
    }

    /* A heading for the groups under it, a step above their titles. */
    .sweep-heading {
        margin: 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
    }
</style>
