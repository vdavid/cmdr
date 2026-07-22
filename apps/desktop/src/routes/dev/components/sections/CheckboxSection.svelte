<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'

    let unchecked = $state(false)
    let checked = $state(true)
    const indeterminate = $state(true)
    let withLabel = $state(false)
</script>

<SectionCard id="components-checkbox" label="Checkbox">
    <div class="grid">
        <div class="cell">
            <p class="caption">Unchecked</p>
            <Checkbox bind:checked={unchecked} ariaLabel="Unchecked example" />
        </div>

        <div class="cell">
            <p class="caption">Checked</p>
            <Checkbox bind:checked ariaLabel="Checked example" />
        </div>

        <div class="cell">
            <p class="caption">Indeterminate</p>
            <Checkbox {indeterminate} ariaLabel="Indeterminate example" />
        </div>

        <div class="cell">
            <p class="caption">Disabled, unchecked</p>
            <Checkbox checked={false} disabled ariaLabel="Disabled unchecked example" />
        </div>

        <div class="cell">
            <p class="caption">Disabled, checked</p>
            <Checkbox checked disabled ariaLabel="Disabled checked example" />
        </div>

        <div class="cell">
            <p class="caption">With an inline label</p>
            <Checkbox bind:checked={withLabel}>Remember this device</Checkbox>
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
