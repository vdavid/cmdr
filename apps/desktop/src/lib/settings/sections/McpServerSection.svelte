<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { checkPortAvailable, findAvailablePort } from '$lib/tauri-commands'
    import { getMatchingSettingIds } from '$lib/settings/settings-search'

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

<div class="section">
    <h2 class="section-title">MCP server</h2>

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
                <button class="check-port-btn" onclick={checkPort} disabled={!mcpEnabled}> Check port </button>
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
                    <button class="use-suggested" onclick={useSuggestedPort}>
                        Use port {suggestedPort} instead
                    </button>
                {/if}
            </div>
        {/if}
    {/if}
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

    .port-setting {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .check-port-btn {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: default;
        white-space: nowrap;
    }

    .check-port-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
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

    .use-suggested {
        padding: 2px 8px;
        border: 1px solid var(--color-accent);
        border-radius: var(--radius-sm);
        background: var(--color-accent);
        color: white;
        font-size: var(--font-size-sm);
        cursor: default;
        transition: background-color var(--transition-base);
    }

    .use-suggested:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }
</style>
