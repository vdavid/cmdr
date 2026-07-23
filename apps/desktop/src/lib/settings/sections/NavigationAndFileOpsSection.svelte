<script lang="ts">
    /**
     * Settings > Behavior > Navigation & file ops.
     *
     * Three cards:
     *   1. **Navigation** — the `behavior.doubleClickPaneNavigatesToParent` switch
     *      (double-click the empty pane background to go up one folder).
     *   2. **File operations** — the file-extension-change confirmation radio. The
     *      conflict/progress settings live in Advanced (their single home); this
     *      page holds only its own settings, never a mirror.
     *   3. **Operation log** — the retention limits (`operationLog.maxAge` /
     *      `operationLog.maxSize`) for the file-operation history and undo log.
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
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
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
    const pasteAsFileDef = getSettingDefinition('fileOperations.pasteClipboardAsFile') ?? defaultDef
    const operationLogMaxAgeDef = getSettingDefinition('operationLog.maxAge') ?? defaultDef
    const operationLogMaxSizeDef = getSettingDefinition('operationLog.maxSize') ?? defaultDef
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

    {#if anyVisible(shouldShow, 'fileOperations.allowFileExtensionChanges', 'fileOperations.pasteClipboardAsFile')}
        <SectionCard label={tString('settings.navigationAndFileOps.card.fileOperations')}>
            {#if shouldShow('fileOperations.allowFileExtensionChanges')}
                <SettingRow
                    id="fileOperations.allowFileExtensionChanges"
                    label={extensionChangesDef.label}
                    description={extensionChangesDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id="fileOperations.allowFileExtensionChanges" />
                </SettingRow>
            {/if}
            {#if shouldShow('fileOperations.pasteClipboardAsFile')}
                <SettingRow
                    id="fileOperations.pasteClipboardAsFile"
                    label={pasteAsFileDef.label}
                    description={pasteAsFileDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id="fileOperations.pasteClipboardAsFile" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, 'operationLog.maxAge', 'operationLog.maxSize')}
        <SectionCard label={tString('settings.navigationAndFileOps.card.operationLog')}>
            <p class="operation-log-intro">{tString('settings.operationLog.intro')}</p>

            {#if shouldShow('operationLog.maxAge')}
                <SettingRow
                    id="operationLog.maxAge"
                    label={operationLogMaxAgeDef.label}
                    description={operationLogMaxAgeDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSelect id="operationLog.maxAge" />
                </SettingRow>
            {/if}

            {#if shouldShow('operationLog.maxSize')}
                <SettingRow
                    id="operationLog.maxSize"
                    label={operationLogMaxSizeDef.label}
                    description={operationLogMaxSizeDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSelect id="operationLog.maxSize" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .operation-log-intro {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }
</style>
