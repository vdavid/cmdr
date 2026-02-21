<script lang="ts">
    import {
        getAdvancedSettings,
        getSetting,
        setSetting,
        resetSetting,
        isModified,
        onSettingChange,
        type SettingId,
        type SettingsValues,
        formatDuration,
    } from '$lib/settings'
    import { Switch } from '@ark-ui/svelte/switch'
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import { searchAdvancedSettings, getMatchIndicesForLabel, highlightMatches } from '$lib/settings/settings-search'
    import { confirmDialog } from '$lib/utils/confirm-dialog'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const allAdvancedSettings = getAdvancedSettings()

    // Reactivity trigger for settings changes
    let settingsChangeCounter = $state(0)

    // Subscribe to setting changes to trigger re-renders
    $effect(() => {
        const unsubscribe = onSettingChange(() => {
            settingsChangeCounter++
        })
        return unsubscribe
    })

    // Filter by search
    const filteredSettings = $derived.by(() => {
        // Depend on change counter to re-evaluate when settings change
        void settingsChangeCounter
        if (!searchQuery.trim()) {
            return allAdvancedSettings
        }
        const results = searchAdvancedSettings(searchQuery)
        return results.map((r) => r.setting)
    })

    async function handleResetAll() {
        const confirmed = await confirmDialog(
            'Reset all advanced settings to their defaults? This cannot be undone.',
            'Reset advanced settings',
        )
        if (confirmed) {
            for (const setting of allAdvancedSettings) {
                resetSetting(setting.id as SettingId)
            }
        }
    }

    function handleBooleanChange(id: SettingId, checked: boolean) {
        setSetting(id, checked as SettingsValues[typeof id])
    }

    function handleNumberChange(id: SettingId, details: NumberInputValueChangeDetails) {
        setSetting(id, details.valueAsNumber as SettingsValues[typeof id])
    }

    function handleReset(id: SettingId) {
        resetSetting(id)
    }

    // Get highlighted label segments for a setting
    function getLabelSegments(label: string, settingId: string) {
        if (!searchQuery.trim()) {
            return [{ text: label, matched: false }]
        }
        const matchIndices = getMatchIndicesForLabel(searchQuery, settingId)
        return highlightMatches(label, matchIndices)
    }
</script>

<div class="section">
    <h2 class="section-title">Advanced</h2>

    <div class="warning-banner">
        <span class="warning-icon">⚠️</span>
        <span>
            These settings are for advanced users. Incorrect values may cause performance issues or unexpected behavior.
        </span>
    </div>

    <div class="header-actions">
        <button class="reset-all-btn" onclick={handleResetAll}>Reset all to defaults</button>
    </div>

    <div class="advanced-settings">
        {#each filteredSettings as setting (`${setting.id}-${String(settingsChangeCounter)}`)}
            {@const id = setting.id as SettingId}
            {@const modified = isModified(id)}
            <div class="advanced-setting-row">
                <div class="setting-info">
                    <div class="setting-name">
                        {#if modified}
                            <span class="modified-dot">●</span>
                        {/if}
                        {#each getLabelSegments(setting.label, setting.id) as segment, i (i)}{#if segment.matched}<mark
                                    class="search-highlight">{segment.text}</mark
                                >{:else}{segment.text}{/if}{/each}
                    </div>
                    <div class="setting-description">{setting.description}</div>
                    <div class="setting-default">
                        Default: {setting.type === 'duration'
                            ? formatDuration(setting.default as number)
                            : String(setting.default)}
                        {#if modified}
                            <button
                                class="reset-link"
                                onclick={() => {
                                    handleReset(id)
                                }}>Reset to default</button
                            >
                        {/if}
                    </div>
                </div>

                <div class="setting-control">
                    {#if setting.type === 'boolean'}
                        <Switch.Root
                            checked={getSetting(id) as boolean}
                            onCheckedChange={(d) => {
                                handleBooleanChange(id, d.checked)
                            }}
                        >
                            <Switch.Control class="switch-control">
                                <Switch.Thumb class="switch-thumb" />
                            </Switch.Control>
                            <Switch.HiddenInput />
                        </Switch.Root>
                    {:else if setting.type === 'number' || setting.type === 'duration'}
                        <NumberInput.Root
                            value={String(getSetting(id))}
                            onValueChange={(d) => {
                                handleNumberChange(id, d)
                            }}
                            min={setting.constraints?.min ?? setting.constraints?.minMs}
                            max={setting.constraints?.max ?? setting.constraints?.maxMs}
                            step={setting.constraints?.step ?? 1}
                        >
                            <NumberInput.Control class="number-control">
                                <NumberInput.DecrementTrigger class="number-btn">−</NumberInput.DecrementTrigger>
                                <NumberInput.Input class="number-input" />
                                <NumberInput.IncrementTrigger class="number-btn">+</NumberInput.IncrementTrigger>
                            </NumberInput.Control>
                        </NumberInput.Root>
                        {#if setting.type === 'duration' && setting.constraints?.unit}
                            <span class="unit">{setting.constraints.unit}</span>
                        {:else if setting.type === 'number'}
                            <span class="unit">
                                {setting.id.includes('Threshold') ? 'px' : setting.id.includes('Buffer') ? 'items' : ''}
                            </span>
                        {/if}
                    {/if}
                </div>
            </div>
        {/each}
    </div>
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }

    .warning-banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm);
        background: rgba(230, 81, 0, 0.1);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
        color: var(--color-warning);
        font-size: var(--font-size-sm);
        margin-bottom: var(--spacing-lg);
    }

    .warning-icon {
        font-size: var(--font-size-lg);
    }

    .header-actions {
        display: flex;
        justify-content: flex-end;
        margin-bottom: var(--spacing-lg);
    }

    .reset-all-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: default;
    }

    .advanced-settings {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        max-height: 500px;
        overflow-y: auto;
    }

    .advanced-setting-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
    }

    .setting-info {
        flex: 1;
        min-width: 0;
    }

    .setting-name {
        font-weight: 500;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-dot {
        color: var(--color-accent);
        font-size: var(--font-size-xs);
    }

    .search-highlight {
        background-color: var(--color-highlight);
        color: inherit;
        padding: 0 2px;
        border-radius: 2px;
    }

    .setting-description {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        margin-top: 2px;
    }

    .setting-default {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin-top: var(--spacing-xs);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .reset-link {
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent);
        font-size: var(--font-size-sm);
        cursor: default;
        text-decoration: underline;
    }

    .setting-control {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        flex-shrink: 0;
    }

    .unit {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        min-width: 40px;
    }

    /* Ark UI component styles */
    :global(.switch-control) {
        display: inline-flex;
        align-items: center;
        width: 36px;
        height: 20px;
        background: var(--color-bg-tertiary);
        border-radius: 10px;
        padding: 2px;
        cursor: default;
        transition: background-color var(--transition-base);
    }

    :global(.switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.switch-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border-radius: var(--radius-full);
        transition: transform var(--transition-base);
        box-shadow: var(--shadow-sm);
    }

    :global(.switch-control[data-state='checked'] .switch-thumb) {
        transform: translateX(16px);
    }

    :global(.number-control) {
        display: flex;
        align-items: center;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        overflow: hidden;
    }

    :global(.number-btn) {
        width: 24px;
        height: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--color-bg-primary);
        border: none;
        color: var(--color-text-primary);
        cursor: default;
        font-size: var(--font-size-sm);
    }

    :global(.number-input) {
        width: 60px;
        padding: var(--spacing-xs);
        border: none;
        border-left: 1px solid var(--color-border);
        border-right: 1px solid var(--color-border);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        text-align: center;
    }

    :global(.number-input:focus) {
        outline: none;
    }
</style>
