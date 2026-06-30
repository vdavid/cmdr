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
    import type { DurationUnit } from '$lib/settings/types'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { Switch } from '@ark-ui/svelte/switch'
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import {
        createShouldShow,
        anyVisible,
        getMatchIndicesForLabel,
        highlightMatches,
    } from '$lib/settings/settings-search'
    import { groupAdvancedByCard } from './advanced-grouping'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const allAdvancedSettings = getAdvancedSettings()

    // The card grouping is computed once (registry-order, stable). Row
    // visibility under search is driven per-row by `shouldShow`, NOT by swapping
    // the settings array, so Advanced rides the SAME search pipeline as every
    // other section (it's now in the global index).
    const cardGroups = groupAdvancedByCard(allAdvancedSettings)

    const shouldShow = $derived(createShouldShow(searchQuery))

    // Reactivity trigger for settings changes
    let settingsChangeCounter = $state(0)

    // Subscribe to setting changes to trigger re-renders
    $effect(() => {
        return onSettingChange(() => {
            settingsChangeCounter++
        })
    })

    async function handleResetAll() {
        const confirmed = await confirmDialog(
            tString('settings.advanced.resetAllConfirm'),
            tString('settings.advanced.resetAllConfirmTitle'),
        )
        if (confirmed) {
            for (const setting of allAdvancedSettings) {
                resetSetting(setting.id)
            }
        }
    }

    function handleBooleanChange(id: SettingId, checked: boolean) {
        setSetting(id, checked as SettingsValues[typeof id])
    }

    const DURATION_UNIT_MS: Record<DurationUnit, number> = {
        ms: 1,
        s: 1000,
        min: 60_000,
        h: 3_600_000,
        d: 86_400_000,
    }

    function unitFactor(unit: DurationUnit | undefined): number {
        return unit ? DURATION_UNIT_MS[unit] : 1
    }

    function handleNumberChange(
        id: SettingId,
        details: NumberInputValueChangeDetails,
        durationUnit?: DurationUnit,
    ) {
        const raw = details.valueAsNumber
        const value = durationUnit ? raw * unitFactor(durationUnit) : raw
        setSetting(id, value as SettingsValues[typeof id])
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

<SettingsSection title={tString('settings.section.advanced')}>
    <div class="warning-banner">
        <span class="warning-icon"><Icon name="triangle-alert" size={18} aria-hidden="true" /></span>
        <span>
            {tString('settings.advanced.warningBanner')}
        </span>
    </div>

    <div class="header-actions">
        <Button variant="secondary" size="mini" onclick={handleResetAll}>{tString('settings.advanced.resetAll')}</Button>
    </div>

    <div class="advanced-settings">
        {#each cardGroups as group (group.title)}
            {@const memberIds = group.settings.map((s) => s.id)}
            {#if anyVisible(shouldShow, ...memberIds)}
                <SectionCard label={group.title || undefined}>
                    {#each group.settings as setting (`${setting.id}-${String(settingsChangeCounter)}`)}
                        {#if shouldShow(setting.id)}
                            {@const id = setting.id}
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
                        {tString('settings.advanced.defaultPrefix')}
                        {setting.type === 'duration'
                            ? formatDuration(Number(setting.default))
                            : String(setting.default)}
                        {#if modified}
                            <button
                                class="reset-link"
                                onclick={() => {
                                    handleReset(id)
                                }}>{tString('settings.control.resetToDefault')}</button
                            >
                        {/if}
                    </div>
                </div>

                <div class="setting-control">
                    {#if setting.type === 'boolean'}
                        <Switch.Root
                            checked={Boolean(getSetting(id))}
                            onCheckedChange={(d) => {
                                handleBooleanChange(id, d.checked)
                            }}
                            aria-label={setting.label}
                        >
                            <Switch.Control class="switch-control">
                                <Switch.Thumb class="switch-thumb" />
                            </Switch.Control>
                            <Switch.HiddenInput />
                        </Switch.Root>
                    {:else if setting.type === 'number' || setting.type === 'duration'}
                        {@const durationUnit =
                            setting.type === 'duration' ? setting.constraints?.unit : undefined}
                        {@const factor = unitFactor(durationUnit)}
                        {@const rawValue = Number(getSetting(id))}
                        <NumberInput.Root
                            value={String(durationUnit ? rawValue / factor : rawValue)}
                            onValueChange={(d) => {
                                handleNumberChange(id, d, durationUnit)
                            }}
                            min={setting.type === 'duration'
                                ? (setting.constraints?.minMs ?? 0) / factor
                                : setting.constraints?.min}
                            max={setting.type === 'duration'
                                ? (setting.constraints?.maxMs ?? Infinity) / factor
                                : setting.constraints?.max}
                            step={setting.constraints?.step ?? 1}
                        >
                            <NumberInput.Control class="number-control">
                                <NumberInput.DecrementTrigger
                                    class="number-btn"
                                    aria-label={tString('settings.control.decrease', { label: setting.label })}
                                    >−</NumberInput.DecrementTrigger
                                >
                                <NumberInput.Input class="number-input" aria-label={setting.label} />
                                <NumberInput.IncrementTrigger
                                    class="number-btn"
                                    aria-label={tString('settings.control.increase', { label: setting.label })}
                                    >+</NumberInput.IncrementTrigger
                                >
                            </NumberInput.Control>
                        </NumberInput.Root>
                        {#if setting.type === 'duration' && setting.constraints?.unit}
                            <span class="unit">{setting.constraints.unit}</span>
                        {:else if setting.type === 'number'}
                            <span class="unit">
                                {setting.id === 'advanced.dragThreshold'
                                    ? 'px'
                                    : setting.id.includes('Buffer')
                                      ? 'items'
                                      : setting.id.endsWith('Mb') || setting.id.includes('DiskSpace')
                                        ? 'MB'
                                        : setting.id === 'fileExplorer.typeToJump.resetDelay'
                                          ? 'ms'
                                          : ''}
                            </span>
                        {/if}
                    {/if}
                                </div>
                            </div>
                        {/if}
                    {/each}
                </SectionCard>
            {/if}
        {/each}
    </div>
</SettingsSection>

<style>
    .warning-banner {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm);
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        margin-bottom: var(--spacing-lg);
    }

    .warning-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-warning-text);
    }

    .header-actions {
        display: flex;
        justify-content: flex-end;
        margin-bottom: var(--spacing-lg);
    }

    .advanced-settings {
        display: flex;
        flex-direction: column;
    }

    /* Rows render inside a `SectionCard` (which owns the background, padding, and
       border), so each row is transparent and full-bleed within the card. A
       subtle divider separates stacked rows in the same card; the first row has
       none (the card's own padding gives the breathing room). */
    .advanced-setting-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-md) 0;
    }

    .advanced-setting-row:first-child {
        padding-top: 0;
    }

    .advanced-setting-row:last-child {
        padding-bottom: 0;
    }

    .advanced-setting-row:not(:last-child) {
        border-bottom: 1px solid var(--color-border-subtle);
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
        color: var(--color-accent-text);
        font-size: var(--font-size-xs);
    }

    .search-highlight {
        background-color: var(--color-highlight);
        color: inherit;
        padding: 0 var(--spacing-xxs);
        border-radius: var(--radius-xs);
    }

    .setting-description {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        margin-top: var(--spacing-xxs);
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
        color: var(--color-accent-text);
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
        border-radius: var(--radius-full);
        padding: var(--spacing-xxs);
        cursor: default;
        transition: background-color var(--transition-base);
    }

    :global(.switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.switch-control[data-state='checked']:hover) {
        background: var(--color-accent-hover);
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

    :global(.number-control:focus-within) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }
</style>
