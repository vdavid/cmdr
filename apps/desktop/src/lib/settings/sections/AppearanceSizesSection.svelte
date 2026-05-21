<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { onMount } from 'svelte'
    import type { FileSizeFormat } from '$lib/settings/types'

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

    // The kilobyte tile reflects the active binary/SI base live: `KB` for
    // binary (1024-based), `kB` for SI (1000-based). MB/GB look the same in
    // both bases so they don't need overrides.
    //
    // Read the format directly from the settings store rather than via
    // `reactive-settings.svelte`'s `getFileSizeFormat()` because the settings
    // window doesn't initialize the reactive layer; only the main window does
    // (`(main)/+layout.svelte`). Subscribing to `onSpecificSettingChange`
    // covers cross-window updates from the settings:changed event the store
    // emits on every write.
    let fileSizeFormat = $state<FileSizeFormat>(getSetting('appearance.fileSizeFormat'))
    onMount(() => onSpecificSettingChange('appearance.fileSizeFormat', (_id, v) => {
        fileSizeFormat = v as FileSizeFormat
    }))
    const sizeUnitLabelOverrides = $derived({
        kB: fileSizeFormat === 'binary' ? 'KB' : 'kB',
    })
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
</SettingsSection>
