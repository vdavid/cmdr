<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getSetting, getSettingDefinition, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { checkPortAvailable, findAvailablePort, setMcpEnabled, setMcpPort } from '$lib/tauri-commands'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import { onMount } from 'svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()
    const log = getAppLogger('mcp-settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const mcpEnabledDef = getSettingDefinition('developer.mcpEnabled') ?? defaultDef
    const mcpPortDef = getSettingDefinition('developer.mcpPort') ?? defaultDef

    const mcpEnabled = $derived(getSetting('developer.mcpEnabled'))
    let serverError = $state<string | null>(null)
    let portStatus = $state<'checking' | 'available' | 'unavailable' | null>(null)
    let suggestedPort = $state<number | null>(null)
    let portDebounceTimer: ReturnType<typeof setTimeout> | undefined
    // Skip exactly one change notification caused by our own revert on failure
    let skipNextEnabledChange = false

    onMount(() => {
        const unsubEnabled = onSpecificSettingChange('developer.mcpEnabled', (_id, value) => {
            if (skipNextEnabledChange) {
                skipNextEnabledChange = false
                return
            }
            void applyMcpEnabled(value)
        })
        const unsubPort = onSpecificSettingChange('developer.mcpPort', (_id, value) => {
            debounceMcpPortChange(value)
        })
        return () => {
            unsubEnabled()
            unsubPort()
            clearTimeout(portDebounceTimer)
        }
    })

    async function applyMcpEnabled(enabled: boolean): Promise<void> {
        serverError = null
        const port = getSetting('developer.mcpPort')
        try {
            await setMcpEnabled(enabled, port)
        } catch (error: unknown) {
            const message = error instanceof Error ? error.message : String(error)
            log.error('Failed to toggle MCP server: {error}', { error: message })
            serverError = message
            // Revert the toggle so it reflects reality
            skipNextEnabledChange = true
            setSetting('developer.mcpEnabled', !enabled)
        }
    }

    function debounceMcpPortChange(port: number): void {
        clearTimeout(portDebounceTimer)
        portDebounceTimer = setTimeout(() => {
            void applyMcpPort(port)
        }, 800)
    }

    async function applyMcpPort(port: number): Promise<void> {
        serverError = null
        try {
            await setMcpPort(port)
        } catch (error: unknown) {
            const message = error instanceof Error ? error.message : String(error)
            log.error('Failed to change MCP port: {error}', { error: message })
            serverError = message
        }
    }

    async function checkPort() {
        const port = getSetting('developer.mcpPort')
        portStatus = 'checking'

        try {
            const available = await checkPortAvailable(port)
            portStatus = available ? 'available' : 'unavailable'

            if (!available) {
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
            {searchQuery}
        >
            <SettingSwitch id="developer.mcpEnabled" />
        </SettingRow>
    {/if}

    {#if serverError}
        <div class="server-error">{serverError}</div>
    {/if}

    {#if shouldShow('developer.mcpPort')}
        <SettingRow
            id="developer.mcpPort"
            label={mcpPortDef.label}
            description={mcpPortDef.description}
            disabled={!mcpEnabled}
            split
            {searchQuery}
        >
            <div class="port-setting">
                <SettingNumberInput id="developer.mcpPort" disabled={!mcpEnabled} />
                <Button variant="secondary" size="mini" onclick={checkPort} disabled={!mcpEnabled}>Check port</Button>
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

    .server-error {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error);
    }
</style>
