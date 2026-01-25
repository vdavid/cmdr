<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'

    interface Props {
        searchQuery: string
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { searchQuery }: Props = $props()

    const verboseLoggingDef = getSettingDefinition('developer.verboseLogging') ?? { label: '', description: '' }

    let copyFeedback = $state(false)

    function openLogFile() {
        // Log files are in the app data directory
        // This would need to be implemented to find the actual log path
        // For now, we'll show a placeholder
        alert('Log file location: ~/Library/Application Support/com.veszelovszki.cmdr/logs/')
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

    <SettingRow
        id="developer.verboseLogging"
        label={verboseLoggingDef.label}
        description={verboseLoggingDef.description}
    >
        <SettingSwitch id="developer.verboseLogging" />
    </SettingRow>

    <div class="logging-actions">
        <button class="action-btn" onclick={openLogFile}>Open log file</button>
        <button class="action-btn" onclick={copyDiagnosticInfo}>
            {copyFeedback ? '✓ Copied!' : 'Copy diagnostic info'}
        </button>
    </div>
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

    .logging-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }

    .action-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: pointer;
    }

    .action-btn:hover {
        background: var(--color-bg-tertiary);
    }
</style>
