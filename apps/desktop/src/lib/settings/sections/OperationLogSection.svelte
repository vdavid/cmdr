<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const maxAgeDef = getSettingDefinition('operationLog.maxAge') ?? defaultDef
    const maxSizeDef = getSettingDefinition('operationLog.maxSize') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.operationLog')}>
    <p class="section-intro">{tString('settings.operationLog.intro')}</p>

    {#if shouldShow('operationLog.maxAge')}
        <SettingRow
            id="operationLog.maxAge"
            label={maxAgeDef.label}
            description={maxAgeDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="operationLog.maxAge" />
        </SettingRow>
    {/if}

    {#if shouldShow('operationLog.maxSize')}
        <SettingRow
            id="operationLog.maxSize"
            label={maxSizeDef.label}
            description={maxSizeDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="operationLog.maxSize" />
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .section-intro {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }
</style>
