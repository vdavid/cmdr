<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { getMatchingSettingIds } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    // Get matching setting IDs for filtering
    const matchingIds = $derived(searchQuery.trim() ? getMatchingSettingIds(searchQuery) : null)

    // Check if a setting should be shown
    function shouldShow(id: string): boolean {
        if (!matchingIds) return true
        return matchingIds.has(id)
    }

    const defaultDef = { label: '', description: '' }
    const cacheDurationDef = getSettingDefinition('network.shareCacheDuration') ?? defaultDef
    const timeoutModeDef = getSettingDefinition('network.timeoutMode') ?? defaultDef
</script>

<div class="section">
    <h2 class="section-title">SMB/Network shares</h2>

    {#if shouldShow('network.shareCacheDuration')}
        <SettingRow
            id="network.shareCacheDuration"
            label={cacheDurationDef.label}
            description={cacheDurationDef.description}
            {searchQuery}
        >
            <SettingSelect id="network.shareCacheDuration" />
        </SettingRow>
    {/if}

    {#if shouldShow('network.timeoutMode')}
        <SettingRow
            id="network.timeoutMode"
            label={timeoutModeDef.label}
            description={timeoutModeDef.description}
            {searchQuery}
        >
            <div class="timeout-setting">
                <SettingRadioGroup id="network.timeoutMode">
                    {#snippet customContent(value)}
                        {#if value === 'custom'}
                            <div class="custom-timeout">
                                <SettingNumberInput id="network.customTimeout" unit="seconds" />
                            </div>
                        {/if}
                    {/snippet}
                </SettingRadioGroup>
            </div>
        </SettingRow>
    {/if}
</div>

<style>
    .section {
        margin-bottom: var(--spacing-md);
    }

    .section-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }

    .timeout-setting {
        width: 100%;
    }

    .custom-timeout {
        margin-top: var(--spacing-xs);
    }
</style>
