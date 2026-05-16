<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const sizeDisplayDef = getSettingDefinition('listing.sizeDisplay') ?? { label: '', description: '' }
    const humanFriendlySizeUnitsDef = getSettingDefinition('listing.humanFriendlySizeUnits') ?? {
        label: '',
        description: '',
    }
    const fileSizeDef = getSettingDefinition('appearance.fileSizeFormat') ?? { label: '', description: '' }
    const sizeMismatchDef = getSettingDefinition('listing.sizeMismatchWarning') ?? { label: '', description: '' }
</script>

<SettingsSection title="File and folder sizes">
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

    {#if shouldShow('appearance.fileSizeFormat')}
        <SettingRow
            id="appearance.fileSizeFormat"
            label={fileSizeDef.label}
            description={fileSizeDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="appearance.fileSizeFormat" />
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
</SettingsSection>
