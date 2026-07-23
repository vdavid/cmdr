<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import NumberInput from '$lib/ui/NumberInput.svelte'

    let plain = $state(5)
    let width = $state(400)
    let stepped = $state(30)
    let disabledValue = $state(12)
</script>

<SectionCard id="components-number-input" label="Number input">
    <div class="grid">
        <div class="cell">
            <p class="caption">Basic, 0–10</p>
            <NumberInput value={plain} onChange={(v: number) => {
                    plain = v
                }} min={0} max={10} ariaLabel="Parallel workers" />
        </div>

        <div class="cell">
            <p class="caption">With a unit, clamped to 250–1000 (typing out of range snaps back on blur)</p>
            <NumberInput
                value={width}
                onChange={(v: number) => {
                    width = v
                }}
                min={250}
                max={1000}
                step={25}
                unit="px"
                ariaLabel="Column width limit"
            />
        </div>

        <div class="cell">
            <p class="caption">Coarse step (15)</p>
            <NumberInput
                value={stepped}
                onChange={(v: number) => {
                    stepped = v
                }}
                min={0}
                max={120}
                step={15}
                unit="min"
                ariaLabel="Check interval"
            />
        </div>

        <div class="cell">
            <p class="caption">Disabled</p>
            <NumberInput
                value={disabledValue}
                onChange={(v: number) => {
                    disabledValue = v
                }}
                min={0}
                max={99}
                ariaLabel="Disabled number input"
                disabled
            />
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
