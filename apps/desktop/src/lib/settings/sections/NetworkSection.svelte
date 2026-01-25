<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSettingDefinition } from '$lib/settings'

    interface Props {
        searchQuery: string
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { searchQuery }: Props = $props()

    const defaultDef = { label: '', description: '' }
    const cacheDurationDef = getSettingDefinition('network.shareCacheDuration') ?? defaultDef
    const timeoutModeDef = getSettingDefinition('network.timeoutMode') ?? defaultDef
</script>

<div class="section">
    <h2 class="section-title">SMB/Network shares</h2>

    <SettingRow
        id="network.shareCacheDuration"
        label={cacheDurationDef.label}
        description={cacheDurationDef.description}
    >
        <SettingSelect id="network.shareCacheDuration" />
    </SettingRow>

    <SettingRow id="network.timeoutMode" label={timeoutModeDef.label} description={timeoutModeDef.description}>
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
