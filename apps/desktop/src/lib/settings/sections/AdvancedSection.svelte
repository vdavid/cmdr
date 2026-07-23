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
    import SettingsSection from '../components/SettingsSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import {
        createShouldShow,
        anyVisible,
        getMatchIndicesForLabel,
        highlightMatches,
    } from '$lib/settings/settings-search'
    import { groupAdvancedByCard, type AdvancedCardGroup } from './advanced-grouping'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { revealItemInDir } from '@tauri-apps/plugin-opener'
    import { appLogDir } from '@tauri-apps/api/path'
    import { getVersion } from '@tauri-apps/api/app'

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

    // Unit label for a plain `number` setting. Duration settings label themselves through
    // `SettingNumberInput` (from their `constraints.unit`); numbers carry no registry unit,
    // so we infer one from the id. Keyed on the stable id, never a user-facing string.
    function numberUnitLabel(id: SettingId): string {
        if (id === 'advanced.dragThreshold') return 'px'
        if (id.includes('Buffer')) return 'items'
        if (id.endsWith('Mb') || id.includes('DiskSpace')) return 'MB'
        if (id === 'fileExplorer.typeToJump.resetDelay') return 'ms'
        return ''
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

    // ------------------------------------------------------------------------
    // Per-card "extra content" (the one bespoke element in the otherwise
    // fully-generated Advanced section).
    //
    // Some cards need trailing non-setting UI after their auto-rendered rows —
    // action buttons that aren't settings and so have no auto-render home. The
    // "Logging and diagnostics" card is one: its verbose-logging switch auto-renders (it's a
    // real `section: ['Advanced']` setting), but the "open log folder" / "copy
    // diagnostics" buttons are actions. A card is matched by a stable MARKER
    // SETTING ID it contains, never by its translated title (no-string-matching).
    // ------------------------------------------------------------------------
    const LOGGING_CARD_MARKER_ID = 'developer.verboseLogging'

    function hasLoggingExtras(group: AdvancedCardGroup): boolean {
        return group.settings.some((s) => s.id === LOGGING_CARD_MARKER_ID)
    }

    let copyFeedback = $state(false)

    async function openLogFile() {
        const log = getAppLogger('settings')
        try {
            const logDir = await appLogDir()
            await revealItemInDir(logDir)
        } catch (error) {
            log.error('Failed to open log directory: {error}', { error: String(error) })
        }
    }

    async function copyDiagnosticInfo() {
        const log = getAppLogger('settings')
        try {
            const info = {
                appVersion: await getVersion(),
                userAgent: navigator.userAgent,
                timestamp: new Date().toISOString(),
            }

            const text = `Cmdr Diagnostic Info
====================
Version: ${info.appVersion}
User Agent: ${info.userAgent}
Timestamp: ${info.timestamp}
`

            await navigator.clipboard.writeText(text)
            copyFeedback = true
            setTimeout(() => {
                copyFeedback = false
            }, 2000)
        } catch (error) {
            log.error('Failed to copy diagnostic info: {error}', { error: String(error) })
        }
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
                        <Switch
                            checked={Boolean(getSetting(id))}
                            onCheckedChange={(next: boolean) => {
                                handleBooleanChange(id, next)
                            }}
                            ariaLabel={setting.label}
                        />
                    {:else if setting.type === 'number' || setting.type === 'duration'}
                        <SettingNumberInput
                            id={setting.id}
                            unit={setting.type === 'number' ? numberUnitLabel(setting.id) : ''}
                        />
                    {/if}
                                </div>
                            </div>
                        {/if}
                    {/each}
                    {#if hasLoggingExtras(group)}
                        <div class="card-extra-actions">
                            <Button variant="secondary" size="mini" onclick={openLogFile}>
                                {tString('settings.logging.openLogFile')}
                            </Button>
                            <Button variant="secondary" size="mini" onclick={copyDiagnosticInfo}>
                                {copyFeedback
                                    ? tString('settings.logging.copied')
                                    : tString('settings.logging.copyDiagnostics')}
                            </Button>
                        </div>
                    {/if}
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

    /* Trailing per-card "extra content" (e.g. the Logging card's action
       buttons), rendered after the card's auto-generated setting rows. */
    .card-extra-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
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

</style>
