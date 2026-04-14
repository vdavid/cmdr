<script lang="ts">
    import { invoke } from '@tauri-apps/api/core'
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import Button from '$lib/ui/Button.svelte'
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
        // Refresh DB size every 2 seconds while visible
        refreshTimer = setInterval(() => void refreshDbSize(), 2000)

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
        <div class="index-row">
            <span class="info-label">Index size</span>
            <div class="index-controls">
                {#if dbFileSize != null || clearing}
                    <Button
                        variant="secondary"
                        size="mini"
                        onclick={handleClearIndex}
                        disabled={clearing}
                    >
                        {clearing ? 'Clearing...' : 'Clear index'}
                    </Button>
                {/if}
                <span class="info-value">
                    {#if dbFileSize != null}
                        {formatFileSize(dbFileSize)}
                    {:else}
                        No index
                    {/if}
                </span>
            </div>
        </div>

        <p class="clear-description">Deletes the index database. A fresh scan starts next time indexing is enabled.</p>

        {#if clearError}
            <div class="clear-error">{clearError}</div>
        {/if}
    </div>
</SettingsSection>

<style>
    .index-info {
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
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

    .clear-error {
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error);
        font-size: var(--font-size-sm);
    }
</style>
