<script lang="ts">
    /**
     * Settings > Behavior > Navigation & file ops.
     *
     * Two cards:
     *   1. **Navigation** — the `behavior.doubleClickPaneNavigatesToParent` switch
     *      (double-click the empty pane background to go up one folder).
     *   2. **File operations** — the file-extension-change confirmation radio. The
     *      conflict/progress settings live in Advanced (their single home); this
     *      page holds only its own settings, never a mirror.
     *
     * Card visibility is section-owned: each `SectionCard` frame is wrapped in
     * `{#if anyVisible(shouldShow, ...ids)}` over the SAME `shouldShow` predicate
     * that gates each row, so an all-filtered-out card hides its frame too (no
     * empty cards under search).
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '', disabled: false, disabledReason: '' }
    const doubleClickDef = getSettingDefinition('behavior.doubleClickPaneNavigatesToParent') ?? defaultDef
    const extensionChangesDef = getSettingDefinition('fileOperations.allowFileExtensionChanges') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.navigationAndFileOps')}>
    {#if anyVisible(shouldShow, 'behavior.doubleClickPaneNavigatesToParent')}
        <SectionCard label={tString('settings.navigationAndFileOps.card.navigation')}>
            {#if shouldShow('behavior.doubleClickPaneNavigatesToParent')}
                <SettingRow
                    id="behavior.doubleClickPaneNavigatesToParent"
                    label={doubleClickDef.label}
                    description={doubleClickDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="behavior.doubleClickPaneNavigatesToParent" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, 'fileOperations.allowFileExtensionChanges')}
        <SectionCard label={tString('settings.navigationAndFileOps.card.fileOperations')}>
            {#if shouldShow('fileOperations.allowFileExtensionChanges')}
                <SettingRow
                    id="fileOperations.allowFileExtensionChanges"
                    label={extensionChangesDef.label}
                    description={extensionChangesDef.description}
                    split
                    {searchQuery}
                >
                    <SettingRadioGroup id="fileOperations.allowFileExtensionChanges" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>
