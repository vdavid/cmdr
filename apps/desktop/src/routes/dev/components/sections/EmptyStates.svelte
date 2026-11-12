<script lang="ts">
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import EmptyFolderState from '#lib/file-explorer/views/EmptyFolderState.svelte'

    /** The file list's empty state, each in a box standing in for a pane's row area. */
    const variants = [0, 1, 12]
</script>

<SectionCard id="components-empty-states" label="Empty states">
    <div class="variants">
        {#each variants as hiddenCount (hiddenCount)}
            <div class="row-area">
                <EmptyFolderState {hiddenCount} />
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .variants {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .row-area {
        position: relative;
        height: 120px;
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
    }
</style>
