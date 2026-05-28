<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ToggleGroup from '$lib/ui/ToggleGroup.svelte'

    let tabsValue = $state('filename')
    let togglesValue = $state('comfortable')
    let withFeaturesValue = $state('cloud')
    let disabledValue = $state('comfortable')

    const tabsOptions = [
        { value: 'ai', label: 'Ask anything', badge: 'AI', hint: '⌥A', ariaLabel: 'AI mode (Alt+A)' },
        { value: 'filename', label: 'Filename', hint: '⌥F', ariaLabel: 'Filename mode (Alt+F)' },
        {
            value: 'content',
            label: 'Content',
            disabled: true,
            tooltip: 'Coming soon: full-text search inside files',
            ariaLabel: 'Content mode (coming soon)',
        },
        { value: 'regex', label: 'Regex', hint: '⌥R', ariaLabel: 'Regex mode (Alt+R)' },
    ]

    const togglesOptions = [
        { value: 'compact', label: 'Compact' },
        { value: 'comfortable', label: 'Comfortable' },
        { value: 'spacious', label: 'Spacious' },
    ]

    const withFeaturesOptions = [
        { value: 'off', label: 'Off' },
        { value: 'local', label: 'Local', hint: '4 GB' },
        { value: 'cloud', label: 'Cloud', badge: 'AI' },
    ]
</script>

<SectionCard id="components-toggle-group" label="Toggle group">
    <div class="grid">
        <div class="cell">
            <p class="caption">Tabs semantics &mdash; badge, hint, disabled with tooltip</p>
            <ToggleGroup
                semantics="tabs"
                value={tabsValue}
                options={tabsOptions}
                onChange={(v) => (tabsValue = v)}
                ariaLabel="Search mode"
            />
        </div>

        <div class="cell">
            <p class="caption">Toggles semantics &mdash; plain labels</p>
            <ToggleGroup
                semantics="toggles"
                value={togglesValue}
                options={togglesOptions}
                onChange={(v) => (togglesValue = v)}
                ariaLabel="UI density"
            />
        </div>

        <div class="cell">
            <p class="caption">Toggles semantics &mdash; with badge and hint</p>
            <ToggleGroup
                semantics="toggles"
                value={withFeaturesValue}
                options={withFeaturesOptions}
                onChange={(v) => (withFeaturesValue = v)}
                ariaLabel="AI provider"
            />
        </div>

        <div class="cell">
            <p class="caption">Toggles semantics &mdash; disabled root</p>
            <ToggleGroup
                semantics="toggles"
                value={disabledValue}
                options={togglesOptions}
                onChange={(v) => (disabledValue = v)}
                ariaLabel="UI density (disabled)"
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
