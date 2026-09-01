<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Size from '$lib/ui/Size.svelte'
    import LabeledRow from '../LabeledRow.svelte'

    // Values picked to land in each rainbow tier: bytes, kB, MB, GB, TB.
    const values: number[] = [500, 50_000, 5_000_000, 5_000_000_000, 5_000_000_000_000]
</script>

<SectionCard id="components-size-badges" label="Size badges">
    <div class="rows">
        <LabeledRow label="Normal">
            <div class="badges">
                {#each values as bytes (bytes)}
                    <Size {bytes} />
                {/each}
            </div>
        </LabeledRow>

        <LabeledRow label="Selected (gold)">
            <div class="badges" data-size-colors="selected">
                {#each values as bytes (`selected-${String(bytes)}`)}
                    <Size {bytes} />
                {/each}
            </div>
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
        flex-wrap: wrap;
        font-size: var(--font-size-md);
    }
</style>
