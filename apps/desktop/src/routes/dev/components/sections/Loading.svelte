<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import LoadingIcon from '$lib/ui/LoadingIcon.svelte'
</script>

<SectionCard id="components-loading" label="Loading">
    <div class="grid">
        <div class="cell">
            <p class="caption">Default</p>
            <div class="icon-host">
                <LoadingIcon />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Opening folder</p>
            <div class="icon-host">
                <LoadingIcon openingFolder />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Loaded count</p>
            <div class="icon-host">
                <LoadingIcon loadedCount={1243} />
            </div>
        </div>
        <div class="cell">
            <p class="caption">Finalizing count</p>
            <div class="icon-host">
                <LoadingIcon finalizingCount={5000} />
            </div>
        </div>
        <div class="cell">
            <p class="caption">With cancel hint</p>
            <div class="icon-host">
                <LoadingIcon showCancelHint />
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: var(--spacing-lg);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    /* LoadingIcon stretches to fill `width: 100%; height: 100%;` so the host
       must give it a finite box. */
    .icon-host {
        height: 120px;
        display: flex;
    }
</style>
