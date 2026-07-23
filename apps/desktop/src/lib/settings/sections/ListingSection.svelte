<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange, type BriefColumnWidthMode } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { onMount } from 'svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const appIconsDef = getSettingDefinition('appearance.useAppIconsForDocuments') ?? { label: '', description: '' }
    const fnKeyBarDef = getSettingDefinition('appearance.showFunctionKeyBar') ?? { label: '', description: '' }
    const dirSortDef = getSettingDefinition('listing.directorySortMode') ?? { label: '', description: '' }
    const showExtInNameDef = getSettingDefinition('listing.showExtensionInName') ?? { label: '', description: '' }
    const showTagsDef = getSettingDefinition('listing.showTags') ?? { label: '', description: '' }
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
    const widthFieldDisabled = $derived(briefWidthMode !== 'limited')
</script>

<SettingsSection title={tString('settings.section.listing')}>
    {#if anyVisible(shouldShow, 'appearance.useAppIconsForDocuments', 'appearance.showFunctionKeyBar', 'listing.directorySortMode', 'listing.showExtensionInName', 'listing.showTags')}
        <SectionCard label={tString('settings.appearance.card.namesAndIcons')}>
            {#if shouldShow('appearance.useAppIconsForDocuments')}
                <SettingRow
                    id="appearance.useAppIconsForDocuments"
                    label={appIconsDef.label}
                    description={appIconsDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="appearance.useAppIconsForDocuments" />
                </SettingRow>
            {/if}
            {#if shouldShow('appearance.showFunctionKeyBar')}
                <SettingRow
                    id="appearance.showFunctionKeyBar"
                    label={fnKeyBarDef.label}
                    description={fnKeyBarDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="appearance.showFunctionKeyBar" />
                </SettingRow>
            {/if}
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
            {#if shouldShow('listing.showExtensionInName')}
                <SettingRow
                    id="listing.showExtensionInName"
                    label={showExtInNameDef.label}
                    description={showExtInNameDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="listing.showExtensionInName" />
                </SettingRow>
            {/if}
            {#if shouldShow('listing.showTags')}
                <SettingRow
                    id="listing.showTags"
                    label={showTagsDef.label}
                    description={showTagsDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="listing.showTags" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, 'listing.briefColumnWidthMode', 'listing.briefColumnWidthMaxPx')}
        <SectionCard label={tString('settings.appearance.card.briefMode')}>
            {#if shouldShow('listing.briefColumnWidthMode') || shouldShow('listing.briefColumnWidthMaxPx')}
                <SettingRow
                    id="listing.briefColumnWidthMode"
                    label={briefWidthModeDef.label}
                    description={briefWidthModeDef.description}
                    {searchQuery}
                >
                    <div class="brief-width-control">
                        <SettingRadioGroup id="listing.briefColumnWidthMode">
                            {#snippet itemTrailing(optionValue: string)}
                                <!-- The width field belongs to the "Limit to" option, so it sits on
                                     that option's line and greys out while the other one is picked. -->
                                {#if optionValue === 'limited'}
                                    <SettingNumberInput
                                        id="listing.briefColumnWidthMaxPx"
                                        unit="px"
                                        disabled={widthFieldDisabled}
                                    />
                                {/if}
                            {/snippet}
                        </SettingRadioGroup>
                    </div>
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .brief-width-control {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 100%;
    }
</style>
