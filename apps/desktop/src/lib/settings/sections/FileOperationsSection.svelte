<script lang="ts">
    /**
     * Settings > Behavior > File operations.
     *
     * One card: the file-extension-change confirmation radio. The conflict and
     * progress settings live in Advanced (their single home); this page holds
     * only its own settings, never a mirror.
     *
     * Card visibility is section-owned: the `SectionCard` frame is wrapped in
     * `{#if anyVisible(shouldShow, ...ids)}` over the SAME `shouldShow` predicate
     * that gates each row, so an all-filtered-out card hides its frame too (no
     * empty cards under search).
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
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
    const extensionChangesDef = getSettingDefinition('fileOperations.allowFileExtensionChanges') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.fileOperations')}>
    {#if anyVisible(shouldShow, 'fileOperations.allowFileExtensionChanges')}
        <SectionCard>
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
