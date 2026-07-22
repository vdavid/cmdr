<script lang="ts">
    /**
     * `Indexing > Drive indexing`: the background file-system indexer. One
     * unlabeled `SectionCard` (the section title already reads "Drive indexing")
     * holding the `indexing.enabled` toggle, the clear-index action (the hidden
     * `indexing.indexSize` search anchor), the per-drive first-connect prompt
     * toggle plus its "re-enable notifications" button, and the stale-drive
     * notification toggle.
     *
     * Stays interactive regardless of the Full Disk Access gate: indexing
     * operates on whatever paths it can read; the gate is for the downloads
     * watcher (which lives in `NotificationsSection.svelte`).
     *
     * Card visibility under search is section-owned: the `SectionCard` frame is
     * wrapped in `{#if anyVisible(shouldShow, ...ids)}` over the SAME `shouldShow`
     * predicate that gates each row, so an all-filtered-out card hides its frame
     * too (no empty cards). The hidden `indexing.indexSize` anchor (its `section`
     * equals this page's) makes "index size" a search hit, and the index-size
     * action row is gated on `shouldShow('indexing.indexSize')`.
     */
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { clearSilencedDrives, hasSilencedDrives } from '$lib/indexing/drive-index-prefs'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Size from '$lib/ui/Size.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { clearDriveIndex, getIndexStatus } from '$lib/tauri-commands'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const enabledDef = getSettingDefinition('indexing.enabled') ?? { label: '', description: '' }
    const askForEachDriveDef = getSettingDefinition('indexing.askForEachDrive') ?? { label: '', description: '' }
    const staleNotifyDef = getSettingDefinition('indexing.staleNotify') ?? { label: '', description: '' }

    // The "Re-enable notifications for all drives" button is disabled until the
    // user has silenced at least one drive's first-connect prompt. Tracked
    // reactively so flipping a silence elsewhere (or here) updates the button.
    let hasSilenced = $state(hasSilencedDrives())

    function handleReEnableNotifications() {
        clearSilencedDrives()
        hasSilenced = false
    }

    let dbFileSize = $state<number | null>(null)
    let clearing = $state(false)
    let clearError = $state<string | null>(null)
    let refreshTimer: ReturnType<typeof setInterval> | undefined

    async function refreshDbSize() {
        const res = await getIndexStatus()
        if (res.status === 'ok') {
            dbFileSize = res.data.dbFileSize
        } else {
            dbFileSize = null
        }
    }

    async function handleClearIndex() {
        clearing = true
        clearError = null
        try {
            const res = await clearDriveIndex()
            if (res.status === 'error') throw new Error(res.error)
            dbFileSize = null
            log.info('Drive index cleared from settings')
        } catch (error: unknown) {
            const msg = error instanceof Error ? error.message : String(error)
            clearError = msg
            log.error('Failed to clear drive index: {error}', { error: msg })
        } finally {
            clearing = false
        }
    }

    onMount(() => {
        void refreshDbSize()
        // Re-read whether any drive is silenced (a first-connect notification can
        // silence one while this page is open, in the same window or another).
        const unsubSilenced = onSpecificSettingChange('indexing.silencedDrives', () => {
            hasSilenced = hasSilencedDrives()
        })
        // Refresh DB size every 2 seconds while visible
        refreshTimer = setInterval(() => void refreshDbSize(), 2000)

        return () => {
            clearInterval(refreshTimer)
            unsubSilenced()
        }
    })
</script>

<SettingsSection title={tString('settings.section.driveIndexing')}>
    {#if anyVisible(shouldShow, 'indexing.enabled', 'indexing.indexSize', 'indexing.askForEachDrive', 'indexing.staleNotify')}
        <SectionCard>
            {#if shouldShow('indexing.enabled')}
                <SettingRow
                    id="indexing.enabled"
                    label={enabledDef.label}
                    description={enabledDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="indexing.enabled" />
                </SettingRow>
            {/if}

            {#if shouldShow('indexing.indexSize')}
                <div class="index-info">
                    <div class="index-row">
                        <span class="info-label">{tString('settings.fileSystemWatching.indexSize')}</span>
                        <div class="index-controls">
                            {#if dbFileSize != null || clearing}
                                <Button variant="secondary" size="mini" onclick={handleClearIndex} disabled={clearing}>
                                    {clearing
                                        ? tString('settings.fileSystemWatching.clearing')
                                        : tString('settings.fileSystemWatching.clearIndex')}
                                </Button>
                            {/if}
                            <span class="info-value">
                                {#if dbFileSize != null}
                                    <Size bytes={dbFileSize} />
                                {:else}
                                    {tString('settings.fileSystemWatching.noIndex')}
                                {/if}
                            </span>
                        </div>
                    </div>

                    <p class="clear-description">
                        {tString('settings.fileSystemWatching.clearIndexDescription')}
                    </p>

                    {#if clearError}
                        <div class="clear-error">{clearError}</div>
                    {/if}
                </div>
            {/if}

            {#if shouldShow('indexing.askForEachDrive')}
                <SettingRow
                    id="indexing.askForEachDrive"
                    label={askForEachDriveDef.label}
                    description={askForEachDriveDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="indexing.askForEachDrive" />
                </SettingRow>
            {/if}

            {#if shouldShow('indexing.askForEachDrive')}
                <div class="reenable-row">
                    <div class="reenable-header">
                        <span class="info-label">{tString('settings.indexing.reEnableNotifications.label')}</span>
                        <span
                            use:tooltip={hasSilenced ? '' : tString('settings.indexing.reEnableNotifications.disabledTooltip')}
                        >
                            <Button
                                variant="secondary"
                                size="mini"
                                disabled={!hasSilenced}
                                onclick={handleReEnableNotifications}
                            >
                                {tString('settings.indexing.reEnableNotifications.button')}
                            </Button>
                        </span>
                    </div>
                    <p class="reenable-description">
                        {tString('settings.indexing.reEnableNotifications.description')}
                    </p>
                </div>
            {/if}

            {#if shouldShow('indexing.staleNotify')}
                <SettingRow
                    id="indexing.staleNotify"
                    label={staleNotifyDef.label}
                    description={staleNotifyDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="indexing.staleNotify" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .index-info {
        padding: var(--spacing-sm) 0;
    }

    .index-row {
        display: grid;
        grid-template-columns: 1fr 1fr;
        align-items: center;
        gap: var(--spacing-md);
    }

    .info-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .index-controls {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
    }

    .info-value {
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .clear-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .reenable-row {
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .reenable-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .reenable-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .clear-error {
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }
</style>
