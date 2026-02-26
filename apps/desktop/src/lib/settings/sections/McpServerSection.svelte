<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { checkPortAvailable, findAvailablePort } from '$lib/tauri-commands'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '', requiresRestart: false }
    const mcpEnabledDef = getSettingDefinition('developer.mcpEnabled') ?? defaultDef
    const mcpPortDef = getSettingDefinition('developer.mcpPort') ?? defaultDef

    const mcpEnabled = $derived(getSetting('developer.mcpEnabled'))
    let portStatus = $state<'checking' | 'available' | 'unavailable' | null>(null)
    let suggestedPort = $state<number | null>(null)

    async function checkPort() {
        const port = getSetting('developer.mcpPort')
        portStatus = 'checking'

        try {
            const available = await checkPortAvailable(port)
            portStatus = available ? 'available' : 'unavailable'

            if (!available) {
                // Find an available port
                suggestedPort = await findAvailablePort(port)
            } else {
                suggestedPort = null
            }
        } catch {
            portStatus = null
        }
    }

    function useSuggestedPort() {
        if (suggestedPort) {
            setSetting('developer.mcpPort', suggestedPort)
            portStatus = 'available'
            suggestedPort = null
        }
    }
</script>

<SettingsSection title="MCP server">
    {#if shouldShow('developer.mcpEnabled')}
        <SettingRow
            id="developer.mcpEnabled"
            label={mcpEnabledDef.label}
            description={mcpEnabledDef.description}
            requiresRestart={mcpEnabledDef.requiresRestart}
            {searchQuery}
        >
            <SettingSwitch id="developer.mcpEnabled" />
        </SettingRow>
    {/if}

    {#if shouldShow('developer.mcpPort')}
        <SettingRow
            id="developer.mcpPort"
            label={mcpPortDef.label}
            description={mcpPortDef.description}
            disabled={!mcpEnabled}
            requiresRestart={mcpPortDef.requiresRestart}
            {searchQuery}
        >
            <div class="port-setting">
                <SettingNumberInput id="developer.mcpPort" disabled={!mcpEnabled} />
                <button class="section-action-btn" onclick={checkPort} disabled={!mcpEnabled}> Check port </button>
            </div>
        </SettingRow>

        {#if portStatus === 'checking'}
            <div class="port-status checking">Checking port availability...</div>
        {:else if portStatus === 'available'}
            <div class="port-status available">Port is available</div>
        {:else if portStatus === 'unavailable'}
            <div class="port-status unavailable">
                Port is in use
                {#if suggestedPort}
                    <Button variant="primary" size="mini" onclick={useSuggestedPort}>
                        Use port {suggestedPort} instead
                    </Button>
                {/if}
            </div>
        {/if}
    {/if}
</SettingsSection>

<style>
    .port-setting {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .port-status {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
        margin-top: var(--spacing-xs);
    }

    .port-status.checking {
        background: var(--color-bg-secondary);
        color: var(--color-text-tertiary);
    }

    .port-status.available {
        background: color-mix(in srgb, var(--color-allow) 10%, transparent);
        color: var(--color-allow);
    }

    .port-status.unavailable {
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
