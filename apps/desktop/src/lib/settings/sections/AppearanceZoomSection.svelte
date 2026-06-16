<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const textSizeDef = getSettingDefinition('appearance.textSize') ?? { label: '', description: '' }
    const uiDensityDef = getSettingDefinition('appearance.uiDensity') ?? { label: '', description: '' }
</script>

<SettingsSection title={tString('settings.section.zoomAndDensity')}>
    {#if shouldShow('appearance.textSize')}
        <SettingRow
            id="appearance.textSize"
            label={textSizeDef.label}
            description={textSizeDef.description}
            split
            {searchQuery}
        >
            <SettingSlider id="appearance.textSize" unit="%" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.uiDensity')}
        <SettingRow
            id="appearance.uiDensity"
            label={uiDensityDef.label}
            description={uiDensityDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="appearance.uiDensity" />
        </SettingRow>
    {/if}
</SettingsSection>
