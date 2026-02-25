<script lang="ts">
    import { invoke } from '@tauri-apps/api/core'
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const enabledDef = getSettingDefinition('indexing.enabled') ?? { label: '', description: '' }

    let dbFileSize = $state<number | null>(null)
    let clearing = $state(false)
    let clearError = $state<string | null>(null)
    let refreshTimer: ReturnType<typeof setInterval> | undefined

    async function refreshDbSize() {
        try {
            const status = await invoke<{
                initialized: boolean
                scanning: boolean
                entriesScanned: number
                dirsFound: number
                indexStatus: unknown
                dbFileSize: number | null
            }>('get_index_status')
            dbFileSize = status.dbFileSize
        } catch {
            dbFileSize = null
        }
    }

    async function handleClearIndex() {
        clearing = true
        clearError = null
        try {
            await invoke('clear_drive_index')
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
        // Refresh DB size every 5 seconds while visible
        refreshTimer = setInterval(() => void refreshDbSize(), 5000)

        return () => {
            clearInterval(refreshTimer)
        }
    })
</script>

<SettingsSection title="Drive indexing">
    {#if shouldShow('indexing.enabled')}
        <SettingRow id="indexing.enabled" label={enabledDef.label} description={enabledDef.description} {searchQuery}>
            <SettingSwitch id="indexing.enabled" />
        </SettingRow>
    {/if}

    <div class="index-info">
        <div class="info-row">
            <span class="info-label">Index size</span>
            <span class="info-value">
                {#if dbFileSize != null}
                    {formatFileSize(dbFileSize)}
                {:else}
                    No index
                {/if}
            </span>
        </div>

        <div class="clear-action">
            <button class="section-action-btn" onclick={handleClearIndex} disabled={clearing || dbFileSize == null}>
                {clearing ? 'Clearing...' : 'Clear index'}
            </button>
            <span class="clear-description">
                Deletes the index database. A fresh scan starts next time indexing is enabled.
            </span>
        </div>

        {#if clearError}
            <div class="clear-error">{clearError}</div>
        {/if}
    </div>
</SettingsSection>

<style>
    .index-info {
        padding: var(--spacing-sm) 0;
    }

    .info-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
    }

    .info-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .info-value {
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .clear-action {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }

    .clear-description {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .clear-error {
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error);
        font-size: var(--font-size-sm);
    }
</style>
