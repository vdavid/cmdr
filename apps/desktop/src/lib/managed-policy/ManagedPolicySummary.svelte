<script lang="ts">
    /**
     * "Managed by your organization": a read-only card listing what the organization's profile
     * restricts, one line per area. Renders nothing when nothing is managed. Settings › Updates &
     * privacy hosts it, so a help-desk person has one place to check that Cmdr honors the profile.
     */
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getManagedPolicyView } from './managed-policy.svelte'
    import { managedPolicySummary } from './policy-summary'

    const lines = $derived(managedPolicySummary(getManagedPolicyView()))
</script>

{#if lines.length > 0}
    <SectionCard label={tString('settings.managed.summary.title')}>
        <dl class="summary">
            {#each lines as line (line.label)}
                <dt>{line.label}</dt>
                <dd>{line.value}</dd>
            {/each}
        </dl>
    </SectionCard>
{/if}

<style>
    /* Two columns, labels left and values right of them, lined up however long a value runs. */
    .summary {
        display: grid;
        grid-template-columns: max-content 1fr;
        gap: var(--spacing-xs) var(--spacing-lg);
        margin: 0;
        font-size: var(--font-size-sm);
    }

    dt {
        color: var(--color-text-secondary);
    }

    dd {
        margin: 0;
        color: var(--color-text-primary);
    }
</style>
