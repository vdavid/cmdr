<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSettingDefinition, getSetting, setSetting } from '$lib/settings'
    import { checkPortAvailable, findAvailablePort } from '$lib/tauri-commands'

    interface Props {
        searchQuery: string
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { searchQuery }: Props = $props()

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
                const found = await findAvailablePort(port)
                suggestedPort = found
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

<div class="section">
    <h2 class="section-title">MCP server</h2>

    <SettingRow
        id="developer.mcpEnabled"
        label={mcpEnabledDef.label}
        description={mcpEnabledDef.description}
        requiresRestart={mcpEnabledDef.requiresRestart}
    >
        <SettingSwitch id="developer.mcpEnabled" />
    </SettingRow>

    <SettingRow
        id="developer.mcpPort"
        label={mcpPortDef.label}
        description={mcpPortDef.description}
        disabled={!mcpEnabled}
        requiresRestart={mcpPortDef.requiresRestart}
    >
        <div class="port-setting">
            <SettingNumberInput id="developer.mcpPort" disabled={!mcpEnabled} />
            <button class="check-port-btn" onclick={checkPort} disabled={!mcpEnabled}> Check port </button>
        </div>
    </SettingRow>

    {#if portStatus === 'checking'}
        <div class="port-status checking">Checking port availability...</div>
    {:else if portStatus === 'available'}
        <div class="port-status available">✓ Port is available</div>
    {:else if portStatus === 'unavailable'}
        <div class="port-status unavailable">
            ✗ Port is in use
            {#if suggestedPort}
                <button class="use-suggested" onclick={useSuggestedPort}>
                    Use port {suggestedPort} instead
                </button>
            {/if}
        </div>
    {/if}
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

    .port-setting {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .check-port-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        white-space: nowrap;
    }

    .check-port-btn:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
    }

    .check-port-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .port-status {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: 4px;
        font-size: var(--font-size-sm);
        margin-top: var(--spacing-xs);
    }

    .port-status.checking {
        background: var(--color-bg-secondary);
        color: var(--color-text-muted);
    }

    .port-status.available {
        background: rgba(46, 125, 50, 0.1);
        color: var(--color-allow);
    }

    .port-status.unavailable {
        background: rgba(211, 47, 47, 0.1);
        color: var(--color-error);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .use-suggested {
        padding: 2px 8px;
        border: 1px solid var(--color-accent);
        border-radius: 4px;
        background: var(--color-accent);
        color: white;
        font-size: var(--font-size-xs);
        cursor: pointer;
    }

    .use-suggested:hover {
        background: var(--color-accent-hover);
    }
</style>
