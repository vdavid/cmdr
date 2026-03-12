<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getSetting, getSettingDefinition, setSetting, onSpecificSettingChange } from '$lib/settings'
    import {
        checkPortAvailable,
        findAvailablePort,
        setMcpEnabled,
        setMcpPort,
        getMcpRunning,
    } from '$lib/tauri-commands'
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

    let serverRunning = $state(false)
    /** The port the running server is actually bound to (may differ from the setting during changes) */
    let runningPort = $state<number | null>(null)
    let serverError = $state<string | null>(null)
    /** Warning shown when the server was stopped due to a port change failure */
    let serverWarning = $state<string | null>(null)
    let portStatus = $state<'checking' | 'available' | 'unavailable' | null>(null)
    let suggestedPort = $state<number | null>(null)
    let portDebounceTimer: ReturnType<typeof setTimeout> | undefined

    // Serialize all MCP operations so rapid toggling can't cause inconsistent state
    let operationQueue = Promise.resolve()

    function enqueue(fn: () => Promise<void>): void {
        operationQueue = operationQueue.then(fn, fn)
    }

    /** Sync toggle + serverRunning from actual backend state */
    async function syncState(): Promise<void> {
        const running = await getMcpRunning()
        serverRunning = running
        if (running) {
            runningPort = getSetting('developer.mcpPort')
        } else {
            runningPort = null
        }
        const settingEnabled = getSetting('developer.mcpEnabled')
        if (settingEnabled !== running) {
            setSetting('developer.mcpEnabled', running)
        }
    }

    onMount(() => {
        void syncState()

        const unsubEnabled = onSpecificSettingChange('developer.mcpEnabled', (_id, value) => {
            // Ignore echoes from our own syncState calls (sync + cross-window).
            // A real user toggle always changes the value away from the current server state.
            if (value === serverRunning) return
            enqueue(() => applyMcpEnabled(value))
        })
        const unsubPort = onSpecificSettingChange('developer.mcpPort', () => {
            debounceMcpPortChange()
        })
        return () => {
            unsubEnabled()
            unsubPort()
            clearTimeout(portDebounceTimer)
        }
    })

    async function applyMcpEnabled(enabled: boolean): Promise<void> {
        serverError = null
        serverWarning = null
        portStatus = null
        suggestedPort = null
        const port = getSetting('developer.mcpPort')
        try {
            await setMcpEnabled(enabled, port)
        } catch (error: unknown) {
            const message = error instanceof Error ? error.message : String(error)
            log.error('Failed to toggle MCP server: {error}', { error: message })
            serverError = message
        }
        await syncState()
    }

    function debounceMcpPortChange(): void {
        // While debouncing, clear stale status so "Server is running on port X" doesn't show the old port
        portStatus = null
        suggestedPort = null
        clearTimeout(portDebounceTimer)
        portDebounceTimer = setTimeout(() => {
            enqueue(() => applyMcpPort())
        }, 800)
    }

    async function applyMcpPort(): Promise<void> {
        const port = getSetting('developer.mcpPort')
        serverError = null
        serverWarning = null
        portStatus = null
        suggestedPort = null

        // Check actual backend state, not the possibly-stale local flag
        const wasRunning = await getMcpRunning()

        if (wasRunning) {
            // Server is running — restart on the new port
            try {
                await setMcpPort(port)
            } catch (error: unknown) {
                const message = error instanceof Error ? error.message : String(error)
                log.error('Failed to change MCP port: {error}', { error: message })
                serverError = message
            }
            await syncState()
            // If the server was stopped because the new port failed, show a warning instead of the raw error
            if (!serverRunning) {
                serverError = null
                serverWarning = `Server turned off because port ${String(port)} is blocked`
            }
            return
        }

        // Server is off — just check availability
        await checkPort()
    }

    async function checkPort(): Promise<void> {
        const port = getSetting('developer.mcpPort')
        portStatus = 'checking'
        suggestedPort = null

        try {
            const available = await checkPortAvailable(port)
            portStatus = available ? 'available' : 'unavailable'

            if (!available) {
                suggestedPort = await findAvailablePort(port)
            }
        } catch {
            portStatus = null
        }
    }

    function useSuggestedPort(): void {
        if (suggestedPort) {
            setSetting('developer.mcpPort', suggestedPort)
            portStatus = null
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

    {#if shouldShow('developer.mcpPort')}
        <SettingRow
            id="developer.mcpPort"
            label={mcpPortDef.label}
            description={mcpPortDef.description}
            split
            {searchQuery}
        >
            <div class="port-setting">
                <SettingNumberInput id="developer.mcpPort" />
                <Button variant="secondary" size="mini" onclick={checkPort}>Check port</Button>
            </div>
        </SettingRow>
    {/if}

    {#if shouldShow('developer.mcpEnabled') || shouldShow('developer.mcpPort')}
        {#if serverError}
            <div class="port-status unavailable">{serverError}</div>
        {:else if serverWarning}
            <div class="port-status warning">{serverWarning}</div>
        {:else if serverRunning && runningPort}
            <div class="port-status active">Server is running on port {runningPort}</div>
        {:else if portStatus === 'checking'}
            <div class="port-status checking">Checking port availability...</div>
        {:else if portStatus === 'available'}
            <div class="port-status available">Port {getSetting('developer.mcpPort')} is available</div>
        {:else if portStatus === 'unavailable'}
            <div class="port-status unavailable">
                Port {getSetting('developer.mcpPort')} is in use
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

    .port-status.available,
    .port-status.active {
        background: color-mix(in srgb, var(--color-allow) 10%, transparent);
        color: var(--color-allow);
    }

    .port-status.warning {
        background: color-mix(in srgb, var(--color-warning) 10%, transparent);
        color: var(--color-warning);
    }

    .port-status.unavailable {
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
