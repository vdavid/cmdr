<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Size from '$lib/ui/Size.svelte'

    // Values picked to land in each rainbow tier: bytes, kB, MB, GB, TB.
    const values: number[] = [500, 50_000, 5_000_000, 5_000_000_000, 5_000_000_000_000]
</script>

<SectionCard id="components-size-badges" label="Size badges">
    <div class="rows">
        <div class="row">
            <span class="row-label">Normal</span>
            <div class="badges">
                {#each values as bytes (bytes)}
                    <Size {bytes} />
                {/each}
            </div>
        </div>

        <div class="row">
            <span class="row-label">Selected (gold)</span>
            <div class="badges" data-size-colors="selected">
                {#each values as bytes (`selected-${String(bytes)}`)}
                    <Size {bytes} />
                {/each}
            </div>
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
        flex-wrap: wrap;
        font-size: var(--font-size-md);
    }
</style>
