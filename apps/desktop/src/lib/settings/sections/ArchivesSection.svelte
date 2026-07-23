<script lang="ts">
    /**
     * Settings > Behavior > Archives.
     *
     * Per-format Enter behavior (Browse | Open | Ask) for archives and macOS app
     * bundles. Two cards:
     *   1. **Archives** — browsable archive formats (zip today; tar/7z join here later).
     *   2. **App bundles** — `.app` / `.bundle` / `.framework` (folders macOS presents
     *      as one item).
     *
     * This is a CUSTOM section (not a registry-driven row): all formats live in ONE
     * pinned-shape JSON setting (`behavior.archiveEnterBehavior`, `{ zip: 'ask', … }`),
     * so the list stays extensible without a registry entry per format. It reads the
     * setting, renders a `ToggleGroup` per format, and writes the merged object back.
     * The pure classification/default logic is `pane/archive-enter-policy.ts`; this
     * file only renders and persists.
     *
     * Card visibility is section-owned (`anyVisible(shouldShow, id)`) over the same
     * `shouldShow` that gates the search — so an all-filtered-out card hides its frame.
     */
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, setSetting, onSpecificSettingChange, getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import {
        ARCHIVE_ENTER_FORMATS,
        parseEnterBehaviorOverrides,
        type ArchiveFormatKey,
        type EnterAction,
    } from '$lib/file-explorer/pane/archive-enter-policy'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const SETTING_ID = 'behavior.archiveEnterBehavior'
    const COMPRESSION_LEVEL_ID = 'behavior.archiveCompressionLevel'

    const compressionLevelDef = getSettingDefinition(COMPRESSION_LEVEL_ID) ?? { label: '', description: '' }

    // Local mirror of the stored JSON, seeded once and kept in sync with the store
    // (a change from another window, or a reset, reflects live).
    let overrides = $state(parseEnterBehaviorOverrides(getSetting(SETTING_ID)))

    onMount(() =>
        onSpecificSettingChange(SETTING_ID, (_id, next) => {
            overrides = parseEnterBehaviorOverrides(next)
        }),
    )

    const options: ToggleGroupOption[] = [
        { value: 'browse', label: tString('settings.archives.opt.browse') },
        { value: 'open', label: tString('settings.archives.opt.open') },
        { value: 'ask', label: tString('settings.archives.opt.ask') },
    ]

    /** The default action for a format, from the pure registry. */
    function defaultAction(key: ArchiveFormatKey): EnterAction {
        return ARCHIVE_ENTER_FORMATS.find((f) => f.key === key)?.defaultAction ?? 'ask'
    }

    /** The effective action for a format: the user's override, or the default. */
    function actionFor(key: ArchiveFormatKey): EnterAction {
        return overrides[key] ?? defaultAction(key)
    }

    function setAction(key: ArchiveFormatKey, action: string): void {
        const next = { ...overrides, [key]: action as EnterAction }
        overrides = next
        setSetting(SETTING_ID, JSON.stringify(next))
    }

    function setZip(action: string): void {
        setAction('zip', action)
    }

    function setBundle(action: string): void {
        setAction('bundle', action)
    }
</script>

<SettingsSection title={tString('settings.section.archives')}>
    {#if anyVisible(shouldShow, SETTING_ID, COMPRESSION_LEVEL_ID)}
        <SectionCard label={tString('settings.archives.card.archives')}>
            {#if shouldShow(SETTING_ID)}
                <div class="archive-row">
                    <div class="archive-label-wrapper">
                        <span class="archive-label">{tString('settings.archives.zip.label')}</span>
                        <p class="archive-description">{tString('settings.archives.zip.description')}</p>
                    </div>
                    <ToggleGroup
                        semantics="toggles"
                        value={actionFor('zip')}
                        {options}
                        onChange={setZip}
                        ariaLabel={tString('settings.archives.zip.label')}
                    />
                </div>
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

    {#if shouldShow(SETTING_ID)}
        <SectionCard label={tString('settings.archives.card.bundles')}>
            <div class="archive-row">
                <div class="archive-label-wrapper">
                    <span class="archive-label">{tString('settings.archives.bundle.label')}</span>
                    <p class="archive-description">{tString('settings.archives.bundle.description')}</p>
                </div>
                <ToggleGroup
                    semantics="toggles"
                    value={actionFor('bundle')}
                    {options}
                    onChange={setBundle}
                    ariaLabel={tString('settings.archives.bundle.label')}
                />
            </div>
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .archive-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) 0;
    }

    .archive-label-wrapper {
        min-width: 0;
    }

    .archive-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .archive-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

</style>
