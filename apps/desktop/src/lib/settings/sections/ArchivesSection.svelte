<script lang="ts">
    /**
     * Settings > Behavior > Archives.
     *
     * What pressing Enter does per format (Browse | Open | Ask), plus the compression
     * level. Two cards:
     *   1. **Archives** — the formats Cmdr can browse into: zip, and the zip-based
     *      documents and app packages (`.docx`/`.jar`/…). Compression level rides
     *      along here, since it's the same "what happens with a zip" topic.
     *   2. **App bundles** — `.app` / `.bundle` / `.framework` (folders macOS presents
     *      as one item).
     *
     * Every row is a registry setting rendered through the house primitives, so this
     * file holds no reading, writing, defaulting, or validating of its own. The format
     * list and the matcher behind each id live in `pane/archive-enter-policy.ts`; the
     * `settingId` there and the entry here are pinned to each other by the parity test
     * in `archive-enter-policy.test.ts`.
     *
     * Card visibility is section-owned (`anyVisible(shouldShow, ...)`) over the same
     * `shouldShow` that gates the search — so an all-filtered-out card hides its frame.
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const ZIP_ID = 'behavior.archiveEnter.zip'
    const OOXML_ID = 'behavior.archiveEnter.ooxml'
    const BUNDLE_ID = 'behavior.archiveEnter.bundle'
    const COMPRESSION_LEVEL_ID = 'behavior.archiveCompressionLevel'

    const emptyDef = { label: '', description: '' }
    const zipDef = getSettingDefinition(ZIP_ID) ?? emptyDef
    const ooxmlDef = getSettingDefinition(OOXML_ID) ?? emptyDef
    const bundleDef = getSettingDefinition(BUNDLE_ID) ?? emptyDef
    const compressionLevelDef = getSettingDefinition(COMPRESSION_LEVEL_ID) ?? emptyDef
</script>

<SettingsSection title={tString('settings.section.archives')}>
    {#if anyVisible(shouldShow, ZIP_ID, OOXML_ID, COMPRESSION_LEVEL_ID)}
        <SectionCard label={tString('settings.archives.card.archives')}>
            {#if shouldShow(ZIP_ID)}
                <SettingRow id={ZIP_ID} label={zipDef.label} description={zipDef.description} {searchQuery}>
                    <SettingToggleGroup id={ZIP_ID} />
                </SettingRow>
            {/if}

            {#if shouldShow(OOXML_ID)}
                <SettingRow id={OOXML_ID} label={ooxmlDef.label} description={ooxmlDef.description} {searchQuery}>
                    <SettingToggleGroup id={OOXML_ID} />
                </SettingRow>
            {/if}

            {#if shouldShow(COMPRESSION_LEVEL_ID)}
                <SettingRow
                    id={COMPRESSION_LEVEL_ID}
                    label={compressionLevelDef.label}
                    description={compressionLevelDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSlider
                        id={COMPRESSION_LEVEL_ID}
                        endLabels={[
                            tString('settings.archives.compressionLevel.faster'),
                            tString('settings.archives.compressionLevel.smaller'),
                        ]}
                    />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, BUNDLE_ID)}
        <SectionCard label={tString('settings.archives.card.bundles')}>
            {#if shouldShow(BUNDLE_ID)}
                <SettingRow id={BUNDLE_ID} label={bundleDef.label} description={bundleDef.description} {searchQuery}>
                    <SettingToggleGroup id={BUNDLE_ID} />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>
