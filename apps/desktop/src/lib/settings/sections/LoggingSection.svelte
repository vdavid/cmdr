<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import Button from '$lib/ui/Button.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import { revealItemInDir } from '@tauri-apps/plugin-opener'
    import { appLogDir } from '@tauri-apps/api/path'
    import { getVersion } from '@tauri-apps/api/app'
    import { tString } from '$lib/intl/messages.svelte'

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
            const log = getAppLogger('settings')
            log.error('Failed to copy diagnostic info: {error}', { error: String(error) })
        }
    }
</script>

<SettingsSection title={tString('settings.section.logging')}>
    {#if anyVisible(shouldShow, 'developer.verboseLogging')}
        <SectionCard>
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
                <Button variant="secondary" size="mini" onclick={openLogFile}
                    >{tString('settings.logging.openLogFile')}</Button
                >
                <Button variant="secondary" size="mini" onclick={copyDiagnosticInfo}>
                    {copyFeedback ? tString('settings.logging.copied') : tString('settings.logging.copyDiagnostics')}
                </Button>
            </div>
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .logging-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-lg);
    }
</style>
