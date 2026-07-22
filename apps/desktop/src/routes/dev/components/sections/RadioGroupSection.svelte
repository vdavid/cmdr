<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import RadioGroup, { type RadioItem } from '$lib/ui/RadioGroup.svelte'

    let basicValue = $state('iso')
    let describedValue = $state('iso')
    let disabledItemValue = $state('auto')
    let horizontalValue = $state('comfortable')

    const basicOptions: RadioItem[] = [
        { value: 'iso', label: 'ISO 8601' },
        { value: 'us', label: 'US' },
        { value: 'eu', label: 'European' },
    ]

    const describedOptions: RadioItem[] = [
        { value: 'iso', label: 'ISO 8601', description: '2025-04-16 10:30' },
        { value: 'us', label: 'US', description: '4/16/2025 10:30 AM' },
        { value: 'custom', label: 'Custom', description: 'Define your own format' },
    ]

    const disabledItemOptions: RadioItem[] = [
        { value: 'auto', label: 'Automatic' },
        { value: 'manual', label: 'Manual' },
        { value: 'off', label: 'Off', disabled: true },
    ]

    const horizontalOptions: RadioItem[] = [
        { value: 'compact', label: 'Compact' },
        { value: 'comfortable', label: 'Comfortable' },
        { value: 'spacious', label: 'Spacious' },
    ]
</script>

<SectionCard id="components-radio-group" label="Radio group">
    <div class="grid">
        <div class="cell">
            <p class="caption">Basic vertical options</p>
            <RadioGroup items={basicOptions} bind:value={basicValue} ariaLabel="Date format" />
        </div>

        <div class="cell">
            <p class="caption">With descriptions</p>
            <RadioGroup items={describedOptions} bind:value={describedValue} ariaLabel="Date format with descriptions" />
        </div>

        <div class="cell">
            <p class="caption">With a disabled option</p>
            <RadioGroup items={disabledItemOptions} bind:value={disabledItemValue} ariaLabel="Sync mode" />
        </div>

        <div class="cell">
            <p class="caption">Horizontal orientation</p>
            <RadioGroup
                items={horizontalOptions}
                bind:value={horizontalValue}
                orientation="horizontal"
                ariaLabel="UI density"
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
