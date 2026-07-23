<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Switch from '$lib/ui/Switch.svelte'

    let off = $state(false)
    let on = $state(true)
    let withLabel = $state(true)
</script>

<SectionCard id="components-switch" label="Switch">
    <div class="grid">
        <div class="cell">
            <p class="caption">Off</p>
            <Switch bind:checked={off} ariaLabel="Off example" />
        </div>

        <div class="cell">
            <p class="caption">On</p>
            <Switch bind:checked={on} ariaLabel="On example" />
        </div>

        <div class="cell">
            <p class="caption">Disabled, off</p>
            <Switch checked={false} disabled ariaLabel="Disabled off example" />
        </div>

        <div class="cell">
            <p class="caption">Disabled, on</p>
            <Switch checked disabled ariaLabel="Disabled on example" />
        </div>

        <div class="cell">
            <p class="caption">With an inline label</p>
            <Switch bind:checked={withLabel}>Search subfolders</Switch>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: var(--spacing-lg);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
