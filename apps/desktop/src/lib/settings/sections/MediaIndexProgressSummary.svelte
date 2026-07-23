<script lang="ts">
    /**
     * The live per-volume image-indexing progress summary, shown inside the "Enable indexing"
     * card in `ImageIndexingSection.svelte` while a pass runs. It reuses the SAME
     * `IndexingEnrichRow` the top-right hourglass renders (per-drive "N of M images", the
     * image + bytes progress bars, and the per-minute rate + ETA), so the two surfaces can't
     * drift. No new backend and no new ETA math: it reads the shared reactive enrichment state
     * (`getEnrichingVolumes`) and resolves drive names from the shared volume store, exactly
     * like `IndexingStatusIndicator`.
     *
     * Renders nothing when no volume is enriching (actively working or paused), so the card
     * stays quiet until there's real progress to show.
     */
    import { getEnrichingVolumes } from '$lib/indexing'
    import IndexingEnrichRow from '$lib/indexing/IndexingEnrichRow.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    const enrichVolumes = $derived(getEnrichingVolumes())
    const volumes = $derived(getVolumes())

    // Resolve a volume id to a human display name from the shared volume store, falling back
    // to the id itself when the volume isn't in the list (honest over blank) — matches
    // `IndexingStatusIndicator`'s `driveName`.
    function driveName(volumeId: string): string {
        return volumes.find((v) => v.id === volumeId)?.name ?? volumeId
    }
</script>

{#if enrichVolumes.length > 0}
    <div class="mi-progress">
        <h4 class="mi-progress-title">{tString('settings.mediaIndex.progressSummary.title')}</h4>
        <div class="mi-progress-rows">
            {#each enrichVolumes as enrich (enrich.volumeId)}
                <IndexingEnrichRow activity={enrich} driveName={driveName(enrich.volumeId)} showHeading={true} />
            {/each}
        </div>
    </div>
{/if}

<style>
    .mi-progress {
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .mi-progress-title {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* A larger gap between drive blocks than within a block (the row's own internal gap is
       `--spacing-xxs`), so multiple drives read as distinct — mirrors the corner tooltip. */
    .mi-progress-rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }
</style>
