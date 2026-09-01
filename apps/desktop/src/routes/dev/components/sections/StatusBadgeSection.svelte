<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import LabeledRow from '../LabeledRow.svelte'

    const statuses = ['alpha', 'beta'] as const
</script>

<SectionCard id="components-status-badge" label="Status badge">
    <div class="rows">
        <LabeledRow label="Statuses">
            <div class="badges">
                {#each statuses as status (status)}
                    <StatusBadge {status} />
                {/each}
            </div>
        </LabeledRow>

        <LabeledRow label="Next to a title">
            <span class="title-sample">Search <StatusBadge status="alpha" /></span>
        </LabeledRow>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .badges {
        display: flex;
        gap: var(--spacing-lg);
        align-items: center;
    }

    .title-sample {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }
</style>
