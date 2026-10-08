<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { unitLabel } from '$lib/units'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const sizeDisplayDef = getSettingDefinition('listing.sizeDisplay') ?? { label: '', description: '' }
    const sizeUnitDef = getSettingDefinition('listing.sizeUnit') ?? { label: '', description: '' }
    const fileSizeDef = getSettingDefinition('appearance.fileSizeFormat') ?? { label: '', description: '' }
    const sizeMismatchDef = getSettingDefinition('listing.sizeMismatchWarning') ?? { label: '', description: '' }
    // `appearance.sizeColors` is registered under `Appearance > Colors and formats`. We
    // render it here too because users hunt for it under "file sizes" just as often.
    // The registry stays single-entry, so search returns one canonical hit (linking to
    // its primary section). `shouldShow` still gates this row when a query is active.
    const sizeColorsDef = getSettingDefinition('appearance.sizeColors') ?? { label: '', description: '' }

    // The unit tiles read in the UI language's symbols (`$lib/units`) and follow the
    // active binary/SI base live (`KiB` / `MiB` / `GiB` for 1024-based, `kB` / `MB` /
    // `GB` for 1000-based), which the registry can't know.
    const sizeUnitLabelOverrides = $derived({
        kB: unitLabel('kB', getFileSizeFormat()),
        MB: unitLabel('MB', getFileSizeFormat()),
        GB: unitLabel('GB', getFileSizeFormat()),
    })
</script>

<SettingsSection title={tString('settings.section.fileAndFolderSizes')}>
    {#if anyVisible(shouldShow, 'listing.sizeDisplay', 'listing.sizeUnit', 'appearance.fileSizeFormat', 'listing.sizeMismatchWarning', 'appearance.sizeColors')}
        <SectionCard>
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

            {#if shouldShow('listing.sizeUnit')}
                <SettingRow
                    id="listing.sizeUnit"
                    label={sizeUnitDef.label}
                    description={sizeUnitDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id="listing.sizeUnit" labelOverrides={sizeUnitLabelOverrides} />
                </SettingRow>
            {/if}

            {#if shouldShow('appearance.fileSizeFormat')}
                <SettingRow
                    id="appearance.fileSizeFormat"
                    label={fileSizeDef.label}
                    description={fileSizeDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id="appearance.fileSizeFormat" />
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

            {#if shouldShow('appearance.sizeColors')}
                <SettingRow
                    id="appearance.sizeColors"
                    label={sizeColorsDef.label}
                    description={sizeColorsDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id="appearance.sizeColors" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>
