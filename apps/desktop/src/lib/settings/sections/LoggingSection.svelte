<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import { revealItemInDir } from '@tauri-apps/plugin-opener'
    import { appLogDir } from '@tauri-apps/api/path'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

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

<SettingsSection title="Logging">
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
        <button class="section-action-btn" onclick={openLogFile}>Open log file</button>
        <button class="section-action-btn" onclick={copyDiagnosticInfo}>
            {copyFeedback ? 'Copied!' : 'Copy diagnostic info'}
        </button>
    </div>
</SettingsSection>

<style>
    .logging-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
    }
</style>
