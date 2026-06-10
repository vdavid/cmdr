<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'

    const statuses = ['alpha', 'beta'] as const
</script>

<SectionCard id="components-status-badge" label="Status badge">
    <div class="rows">
        <div class="row">
            <span class="row-label">Statuses</span>
            <div class="badges">
                {#each statuses as status (status)}
                    <StatusBadge {status} />
                {/each}
            </div>
        </div>

        <div class="row">
            <span class="row-label">Next to a title</span>
            <span class="title-sample">Search <StatusBadge status="alpha" /></span>
        </div>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .row {
        display: flex;
        gap: var(--spacing-lg);
        align-items: center;
        flex-wrap: wrap;
    }

    .row-label {
        min-width: 140px;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
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
