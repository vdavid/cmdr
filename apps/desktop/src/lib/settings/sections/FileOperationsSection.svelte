<script lang="ts">
    /**
     * Settings > Behavior > File operations.
     *
     * Two card groups:
     *   1. **Renaming** — the file-extension-change confirmation radio.
     *   2. **Conflicts and progress** — `fileOperations.maxConflictsToShow` and
     *      `fileOperations.progressUpdateInterval`. Both are `showInAdvanced`
     *      mirrors: they keep their Advanced presence AND render here, their
     *      natural page, so a future globally-searchable Advanced doesn't match
     *      this page and then show a blank section. See `lib/settings/CLAUDE.md`
     *      § "Mirroring a setting in multiple sections".
     *
     * Card visibility is section-owned: each `SectionCard` frame is wrapped in
     * `{#if anyVisible(shouldShow, ...ids)}` over the SAME `shouldShow` predicate
     * that gates each row, so an all-filtered-out card hides its frame too (no
     * empty cards under search).
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
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
    const maxConflictsDef = getSettingDefinition('fileOperations.maxConflictsToShow') ?? defaultDef
    const progressIntervalDef = getSettingDefinition('fileOperations.progressUpdateInterval') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.fileOperations')}>
    {#if anyVisible(shouldShow, 'fileOperations.allowFileExtensionChanges')}
        <SectionCard label={tString('settings.fileOperations.card.renaming')}>
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

    {#if anyVisible(shouldShow, 'fileOperations.maxConflictsToShow', 'fileOperations.progressUpdateInterval')}
        <SectionCard label={tString('settings.fileOperations.card.conflictsAndProgress')}>
            {#if shouldShow('fileOperations.maxConflictsToShow')}
                <SettingRow
                    id="fileOperations.maxConflictsToShow"
                    label={maxConflictsDef.label}
                    description={maxConflictsDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSelect id="fileOperations.maxConflictsToShow" />
                </SettingRow>
            {/if}
            {#if shouldShow('fileOperations.progressUpdateInterval')}
                <SettingRow
                    id="fileOperations.progressUpdateInterval"
                    label={progressIntervalDef.label}
                    description={progressIntervalDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSlider id="fileOperations.progressUpdateInterval" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>
