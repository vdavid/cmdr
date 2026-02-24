<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { getMatchingSettingIds } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import { revealItemInDir } from '@tauri-apps/plugin-opener'
    import { appLogDir } from '@tauri-apps/api/path'

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

    const verboseLoggingDef = getSettingDefinition('developer.verboseLogging') ?? { label: '', description: '' }

    let copyFeedback = $state(false)

    async function openLogFile() {
        try {
            const logDir = await appLogDir()
            await revealItemInDir(logDir)
        } catch (error) {
            const log = getAppLogger('settings')
            log.error('Failed to open log directory: {error}', { error: String(error) })
        }
    }

    async function copyDiagnosticInfo() {
        const info = {
            appVersion: '0.3.2', // Could be fetched from package.json
            userAgent: navigator.userAgent,
            timestamp: new Date().toISOString(),
        }

        const text = `Cmdr Diagnostic Info
====================
Version: ${info.appVersion}
User Agent: ${info.userAgent}
Timestamp: ${info.timestamp}
`

        try {
            await navigator.clipboard.writeText(text)
            copyFeedback = true
            setTimeout(() => {
                copyFeedback = false
            }, 2000)
        } catch (error) {
            // eslint-disable-next-line no-console
            console.error('Failed to copy diagnostic info:', error)
        }
    }
</script>

<div class="section">
    <h2 class="section-title">Logging</h2>

    {#if shouldShow('developer.verboseLogging')}
        <SettingRow
            id="developer.verboseLogging"
            label={verboseLoggingDef.label}
            description={verboseLoggingDef.description}
            {searchQuery}
        >
            <SettingSwitch id="developer.verboseLogging" />
        </SettingRow>
    {/if}

    <div class="logging-actions">
        <button class="action-btn" onclick={openLogFile}>Open log file</button>
        <button class="action-btn" onclick={copyDiagnosticInfo}>
            {copyFeedback ? 'Copied!' : 'Copy diagnostic info'}
        </button>
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

    .logging-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
    }

    .action-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: default;
    }
</style>
