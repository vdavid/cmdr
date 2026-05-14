<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange, type BriefColumnWidthMode } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { onMount } from 'svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const dirSortDef = getSettingDefinition('listing.directorySortMode') ?? { label: '', description: '' }
    const sizeDisplayDef = getSettingDefinition('listing.sizeDisplay') ?? { label: '', description: '' }
    const humanFriendlySizeUnitsDef = getSettingDefinition('listing.humanFriendlySizeUnits') ?? {
        label: '',
        description: '',
    }
    const sizeMismatchDef = getSettingDefinition('listing.sizeMismatchWarning') ?? { label: '', description: '' }
    const stripedRowsDef = getSettingDefinition('listing.stripedRows') ?? { label: '', description: '' }
    const briefWidthModeDef = getSettingDefinition('listing.briefColumnWidthMode') ?? { label: '', description: '' }

    // Read the setting directly and subscribe in-window. `reactive-settings.svelte.ts` is only
    // initialised in the main window. The settings window has its own JS context where that
    // module-scope state never updates, so we can't rely on its getter here.
    let briefWidthMode = $state<BriefColumnWidthMode>(getSetting('listing.briefColumnWidthMode'))
    onMount(() =>
        onSpecificSettingChange('listing.briefColumnWidthMode', (_id, value) => {
            briefWidthMode = value
        }),
    )
    const sliderDisabled = $derived(briefWidthMode !== 'limited')
</script>

<SettingsSection title="Listing">
    {#if shouldShow('listing.directorySortMode')}
        <SettingRow
            id="listing.directorySortMode"
            label={dirSortDef.label}
            description={dirSortDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="listing.directorySortMode" />
        </SettingRow>
    {/if}
    {#if shouldShow('listing.sizeDisplay')}
        <SettingRow
            id="listing.sizeDisplay"
            label={sizeDisplayDef.label}
            description={sizeDisplayDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="listing.sizeDisplay" />
        </SettingRow>
    {/if}
    {#if shouldShow('listing.humanFriendlySizeUnits')}
        <SettingRow
            id="listing.humanFriendlySizeUnits"
            label={humanFriendlySizeUnitsDef.label}
            description={humanFriendlySizeUnitsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="listing.humanFriendlySizeUnits" />
        </SettingRow>
    {/if}
    {#if shouldShow('listing.sizeMismatchWarning')}
        <SettingRow
            id="listing.sizeMismatchWarning"
            label={sizeMismatchDef.label}
            description={sizeMismatchDef.description}
            {searchQuery}
        >
            <SettingSwitch id="listing.sizeMismatchWarning" />
        </SettingRow>
    {/if}
    {#if shouldShow('listing.stripedRows')}
        <SettingRow
            id="listing.stripedRows"
            label={stripedRowsDef.label}
            description={stripedRowsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="listing.stripedRows" />
        </SettingRow>
    {/if}
    {#if shouldShow('listing.briefColumnWidthMode')}
        <SettingRow
            id="listing.briefColumnWidthMode"
            label={briefWidthModeDef.label}
            description={briefWidthModeDef.description}
            {searchQuery}
        >
            <div class="brief-width-control">
                <SettingRadioGroup id="listing.briefColumnWidthMode" />
                <div class="slider-row" class:is-disabled={sliderDisabled}>
                    <SettingSlider id="listing.briefColumnWidthMaxPx" unit="px" disabled={sliderDisabled} />
                </div>
            </div>
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .brief-width-control {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 100%;
    }

    .slider-row {
        /* Visually nests the slider under the radio choices. */
        padding-left: var(--spacing-xl);
    }

    .slider-row.is-disabled {
        opacity: 0.5;
    }
</style>
