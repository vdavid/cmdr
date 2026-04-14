<script lang="ts">
    import { onDestroy } from 'svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingPasswordInput from '../components/SettingPasswordInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        getCloudProvider,
        getProviderConfigs,
        setProviderConfig,
        resolveCloudConfig,
        cloudProviderPresets,
    } from '$lib/settings'
    import { checkAiConnection, configureAi } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'

    interface Props {
        searchQuery: string
        shouldShow: (id: string) => boolean
    }

    const { searchQuery, shouldShow }: Props = $props()

    const logger = getAppLogger('ai-settings-cloud')

    // Cloud provider state
    let cloudProviderId = $state(getSetting('ai.cloudProvider'))
    let currentApiKey = $state('')
    let currentModel = $state('')
    let currentBaseUrl = $state('')

    // Connection check state
    type ConnectionStatus =
        | 'idle'
        | 'checking'
        | 'connected'
        | 'connected-no-models'
        | 'auth-error'
        | 'connection-error'
        | 'error'
    let connectionStatus = $state<ConnectionStatus>('idle')
    let connectionError = $state<string | null>(null)
    let availableModels = $state<string[]>([])
    let connectionCheckTimer: ReturnType<typeof setTimeout> | null = null

    // Model combobox state
    let comboboxOpen = $state(false)
    let comboboxFilter = $state('')
    let highlightedIndex = $state(-1)

    const filteredModels = $derived(
        comboboxFilter
            ? availableModels.filter((m) => m.toLowerCase().includes(comboboxFilter.toLowerCase()))
            : availableModels,
    )

    // Event listeners cleanup
    const unlistenFns: Array<() => void> = []

    // Migrate old settings to new per-provider config if needed
    migrateOldSettings()

    // Load current cloud provider config into local state
    loadCloudProviderConfig(cloudProviderId)

    // Subscribe to cloud provider changes
    const unsubCloudProvider = onSpecificSettingChange('ai.cloudProvider', (_id, newValue) => {
        cloudProviderId = newValue
        loadCloudProviderConfig(cloudProviderId)
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudProvider)

    // Subscribe to cloud provider configs changes (push to backend)
    const unsubCloudConfigs = onSpecificSettingChange('ai.cloudProviderConfigs', () => {
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudConfigs)

    onDestroy(() => {
        for (const fn of unlistenFns) {
            fn()
        }
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
        }
    })

    async function pushConfigToBackend(): Promise<void> {
        try {
            const resolved = resolveCloudConfig(getSetting('ai.cloudProvider'), getSetting('ai.cloudProviderConfigs'))
            await configureAi(
                getSetting('ai.provider'),
                Number(getSetting('ai.localContextSize')),
                resolved.apiKey,
                resolved.baseUrl,
                resolved.model,
            )
        } catch (e) {
            logger.error("Couldn't push AI config to backend: {error}", { error: e })
        }
    }

    function scheduleConnectionCheck(delayMs: number = 1000): void {
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
        }
        connectionCheckTimer = setTimeout(() => {
            connectionCheckTimer = null
            void triggerConnectionCheck()
        }, delayMs)
    }

    async function triggerConnectionCheck(): Promise<void> {
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
            connectionCheckTimer = null
        }

        if (!hasCheckableConfig) return

        connectionStatus = 'checking'
        connectionError = null
        availableModels = []

        try {
            const result = await checkAiConnection(resolvedBaseUrl, currentApiKey)

            if (result.authError) {
                connectionStatus = 'auth-error'
                connectionError = result.error
            } else if (!result.connected) {
                connectionStatus = 'connection-error'
                connectionError = result.error
            } else if (result.error) {
                connectionStatus = 'error'
                connectionError = result.error
            } else if (result.models.length > 0) {
                connectionStatus = 'connected'
                availableModels = result.models
            } else {
                connectionStatus = 'connected-no-models'
            }
        } catch (e) {
            connectionStatus = 'error'
            connectionError = e instanceof Error ? e.message : 'Unknown error'
        }
    }

    function resetConnectionState(): void {
        connectionStatus = 'idle'
        connectionError = null
        availableModels = []
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
            connectionCheckTimer = null
        }
    }

    function migrateOldSettings(): void {
        const oldApiKey = getSetting('ai.openaiApiKey')
        const oldConfigs = getSetting('ai.cloudProviderConfigs')

        if (oldApiKey && oldConfigs === '{}') {
            const oldBaseUrl = getSetting('ai.openaiBaseUrl')
            const oldModel = getSetting('ai.openaiModel')

            // Detect which provider the old base URL matches
            const matchedPreset = cloudProviderPresets.find(
                (p) => p.id !== 'custom' && p.baseUrl && oldBaseUrl.startsWith(p.baseUrl.replace(/\/$/, '')),
            )
            const detectedId = matchedPreset?.id ?? 'custom'

            const config = {
                apiKey: oldApiKey,
                model: oldModel,
                ...(detectedId === 'custom' || detectedId === 'azure-openai' ? { baseUrl: oldBaseUrl } : {}),
            }

            setSetting('ai.cloudProvider', detectedId)
            setSetting('ai.cloudProviderConfigs', setProviderConfig('{}', detectedId, config))
            cloudProviderId = detectedId

            logger.info('Migrated old OpenAI settings to cloud provider config: {provider}', { provider: detectedId })
        }
    }

    function loadCloudProviderConfig(providerId: string): void {
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const providerConfig = configs[providerId]
        const preset = getCloudProvider(providerId)

        currentApiKey = providerConfig?.apiKey ?? ''
        currentModel = providerConfig?.model ?? preset?.defaultModel ?? ''
        currentBaseUrl =
            providerId === 'custom' || providerId === 'azure-openai'
                ? (providerConfig?.baseUrl ?? preset?.baseUrl ?? '')
                : (preset?.baseUrl ?? '')
    }

    function saveCloudProviderField(field: 'apiKey' | 'model' | 'baseUrl', value: string): void {
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const existing = configs[cloudProviderId] ?? { apiKey: '', model: '' }

        if (field === 'apiKey') existing.apiKey = value
        else if (field === 'model') existing.model = value
        else existing.baseUrl = value

        const newJson = setProviderConfig(configsJson, cloudProviderId, existing)
        setSetting('ai.cloudProviderConfigs', newJson)

        // Trigger debounced connection check on API key or base URL change
        if (field === 'apiKey' || field === 'baseUrl') {
            scheduleConnectionCheck()
        }
    }

    function handleCloudProviderChange(newProviderId: string): void {
        setSetting('ai.cloudProvider', newProviderId)
        // Reset and re-check with new provider config
        resetConnectionState()
        // Trigger immediate check after provider config loads (next tick)
        setTimeout(() => {
            if (hasCheckableConfig) {
                void triggerConnectionCheck()
            }
        }, 0)
    }

    function selectModel(model: string): void {
        currentModel = model
        comboboxFilter = ''
        comboboxOpen = false
        highlightedIndex = -1
        saveCloudProviderField('model', model)
    }

    function handleComboboxKeydown(e: KeyboardEvent): void {
        if (!comboboxOpen && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
            comboboxOpen = true
            highlightedIndex = 0
            e.preventDefault()
            return
        }
        if (!comboboxOpen) return

        if (e.key === 'ArrowDown') {
            e.preventDefault()
            highlightedIndex = Math.min(highlightedIndex + 1, filteredModels.length - 1)
        } else if (e.key === 'ArrowUp') {
            e.preventDefault()
            highlightedIndex = Math.max(highlightedIndex - 1, 0)
        } else if (e.key === 'Enter' && highlightedIndex >= 0 && highlightedIndex < filteredModels.length) {
            e.preventDefault()
            selectModel(filteredModels[highlightedIndex])
        } else if (e.key === 'Escape') {
            comboboxOpen = false
            highlightedIndex = -1
        }
    }

    function handleComboboxBlur(): void {
        // Delay to allow click events on dropdown options to fire first
        setTimeout(() => {
            comboboxOpen = false
            highlightedIndex = -1
            comboboxFilter = ''
        }, 150)
    }

    // Derived state
    const currentPreset = $derived(getCloudProvider(cloudProviderId))
    const showEditableBaseUrl = $derived(cloudProviderId === 'custom' || cloudProviderId === 'azure-openai')
    const resolvedBaseUrl = $derived(showEditableBaseUrl ? currentBaseUrl : (currentPreset?.baseUrl ?? ''))
    const requiresApiKey = $derived(currentPreset?.requiresApiKey ?? false)
    const hasCheckableConfig = $derived(requiresApiKey ? currentApiKey !== '' : resolvedBaseUrl !== '')
    const apiKeyPlaceholder = $derived(
        cloudProviderId === 'openai'
            ? 'Example: sk-abc123...'
            : cloudProviderId === 'anthropic'
              ? 'Example: sk-ant-abc123...'
              : 'API key',
    )
</script>

{#if shouldShow('ai.cloudProvider')}
    <SettingRow
        id="ai.cloudProvider"
        label="Service"
        description="Which cloud AI service to use."
        split
        {searchQuery}
    >
        <select
            class="cloud-provider-select"
            value={cloudProviderId}
            onchange={(e: Event) => {
                const target = e.target as HTMLSelectElement
                handleCloudProviderChange(target.value)
            }}
            aria-label="Cloud AI service"
        >
            {#each cloudProviderPresets as preset (preset.id)}
                <option value={preset.id}>{preset.name}</option>
            {/each}
        </select>
    </SettingRow>
    {#if currentPreset?.description}
        <p class="provider-description">{currentPreset.description}</p>
    {/if}
{/if}

<SettingRow
    id="ai.cloudProviderConfigs"
    label="Endpoint"
    description="API endpoint URL for the selected service."
    split
    {searchQuery}
>
    {#if showEditableBaseUrl}
        <input
            class="text-input"
            type="text"
            value={currentBaseUrl}
            oninput={(e: Event) => {
                const target = e.target as HTMLInputElement
                currentBaseUrl = target.value
                saveCloudProviderField('baseUrl', target.value)
            }}
            placeholder="Example: https://api.example.com/v1"
            aria-label="Endpoint URL"
            autocomplete="off"
            spellcheck="false"
        />
    {:else}
        <input
            class="text-input text-input-readonly"
            type="text"
            value={resolvedBaseUrl}
            readonly
            aria-label="Endpoint URL"
            tabindex="-1"
        />
    {/if}
</SettingRow>

{#if requiresApiKey}
    <SettingRow
        id="ai.cloudProviderConfigs"
        label="API key"
        description="Your API key for this service."
        split
        {searchQuery}
    >
        <SettingPasswordInput
            id="ai.cloudProviderConfigs"
            placeholder={apiKeyPlaceholder}
            ariaLabel="API key"
            value={currentApiKey}
            onchange={(value: string) => {
                currentApiKey = value
                saveCloudProviderField('apiKey', value)
            }}
        />
    </SettingRow>
{/if}

<SettingRow
    id="ai.cloudProviderConfigs"
    label="Model"
    description="The model name to use for completions."
    split
    {searchQuery}
>
    {#if availableModels.length > 0}
        <div class="combobox-wrapper">
            <div class="combobox-input-wrapper">
                <input
                    class="text-input combobox-input"
                    type="text"
                    value={comboboxOpen ? comboboxFilter : currentModel}
                    onfocus={() => {
                        comboboxOpen = true
                        comboboxFilter = ''
                        highlightedIndex = -1
                    }}
                    onblur={handleComboboxBlur}
                    oninput={(e: Event) => {
                        const target = e.target as HTMLInputElement
                        comboboxFilter = target.value
                        currentModel = target.value
                        highlightedIndex = 0
                        saveCloudProviderField('model', target.value)
                    }}
                    onkeydown={handleComboboxKeydown}
                    placeholder={currentPreset?.defaultModel
                        ? `Example: ${currentPreset.defaultModel}`
                        : 'Model name'}
                    aria-label="Model"
                    aria-controls="model-listbox"
                    aria-expanded={comboboxOpen}
                    aria-haspopup="listbox"
                    autocomplete="off"
                    spellcheck="false"
                    role="combobox"
                />
                <button
                    class="combobox-toggle"
                    tabindex="-1"
                    aria-label="Show models"
                    onmousedown={(e: MouseEvent) => {
                        e.preventDefault()
                        comboboxOpen = !comboboxOpen
                    }}
                >
                    &#x25BE;
                </button>
            </div>
            {#if comboboxOpen}
                <div class="combobox-dropdown" role="listbox" id="model-listbox">
                    {#if filteredModels.length === 0}
                        <div class="combobox-empty">No matching models</div>
                    {:else}
                        {#each filteredModels as model, i (model)}
                            <div
                                class="combobox-option"
                                class:highlighted={i === highlightedIndex}
                                class:selected={model === currentModel}
                                role="option"
                                aria-selected={model === currentModel}
                                onmousedown={(e: MouseEvent) => {
                                    e.preventDefault()
                                    selectModel(model)
                                }}
                                onmouseenter={() => {
                                    highlightedIndex = i
                                }}
                            >
                                {model}
                            </div>
                        {/each}
                    {/if}
                </div>
            {/if}
        </div>
    {:else}
        <input
            class="text-input"
            type="text"
            value={currentModel}
            oninput={(e: Event) => {
                const target = e.target as HTMLInputElement
                currentModel = target.value
                saveCloudProviderField('model', target.value)
            }}
            placeholder={currentPreset?.defaultModel
                ? `Example: ${currentPreset.defaultModel}`
                : 'Model name'}
            aria-label="Model"
            autocomplete="off"
            spellcheck="false"
        />
    {/if}
</SettingRow>

<!-- Connection status -->
{#if connectionStatus === 'checking'}
    <div class="connection-status">
        <span class="status-spinner">&#x27F3;</span>
        <span class="connection-status-text">Checking...</span>
    </div>
{:else if connectionStatus === 'connected'}
    <div class="connection-status">
        <span class="connection-status-icon connection-status-ok">&#x2713;</span>
        <span class="connection-status-text">Connected</span>
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Recheck</Button>
    </div>
{:else if connectionStatus === 'connected-no-models'}
    <div class="connection-status">
        <span class="connection-status-icon connection-status-ok">&#x2713;</span>
        <span class="connection-status-text">Connected (model list not available)</span>
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Recheck</Button>
    </div>
{:else if connectionStatus === 'auth-error'}
    <div class="connection-status">
        <span class="connection-status-icon connection-status-error">&#x2717;</span>
        <span class="connection-status-text connection-status-error-text"
            >{connectionError ?? 'API key is invalid'}</span
        >
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Recheck</Button>
    </div>
{:else if connectionStatus === 'connection-error'}
    <div class="connection-status">
        <span class="connection-status-icon connection-status-error">&#x2717;</span>
        <span class="connection-status-text connection-status-error-text"
            >{connectionError ?? "Can't reach server"}</span
        >
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Recheck</Button>
    </div>
{:else if connectionStatus === 'error'}
    <div class="connection-status">
        <span class="connection-status-icon connection-status-error">&#x2717;</span>
        <span class="connection-status-text connection-status-error-text"
            >{connectionError ?? 'Something went wrong'}</span
        >
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Recheck</Button>
    </div>
{:else if connectionStatus === 'idle' && hasCheckableConfig}
    <div class="connection-status">
        <Button size="mini" onclick={() => void triggerConnectionCheck()}>Test connection</Button>
    </div>
{/if}

<style>
    /* Cloud provider select */
    .cloud-provider-select {
        width: 100%;
        padding: var(--spacing-sm) var(--spacing-md);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.4;
        cursor: default;
        transition: border-color var(--transition-base);
    }

    .cloud-provider-select:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .provider-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
        line-height: 1.4;
    }

    /* Text input (same style as other setting inputs) */
    .text-input {
        width: 100%;
        padding: var(--spacing-sm) var(--spacing-md);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.4;
        transition: border-color var(--transition-base);
    }

    .text-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .text-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .text-input-readonly {
        opacity: 0.7;
        cursor: default;
    }

    .text-input-readonly:focus {
        border-color: var(--color-border);
        box-shadow: none;
    }

    /* Connection status */
    .connection-status {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        font-size: var(--font-size-sm);
    }

    .status-spinner {
        display: inline-block;
        animation: spin 1s linear infinite;
        color: var(--color-text-secondary);
    }

    .connection-status-icon {
        font-weight: 600;
    }

    .connection-status-ok {
        color: var(--color-allow);
    }

    .connection-status-error {
        color: var(--color-error);
    }

    .connection-status-error-text {
        color: var(--color-error);
    }

    .connection-status-text {
        color: var(--color-text-secondary);
    }

    /* Model combobox */
    .combobox-wrapper {
        position: relative;
        width: 100%;
    }

    .combobox-input-wrapper {
        position: relative;
        display: flex;
        align-items: center;
    }

    .combobox-input {
        padding-right: var(--spacing-2xl);
    }

    .combobox-toggle {
        position: absolute;
        right: 6px;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        cursor: default;
        font-size: var(--font-size-sm);
        padding: var(--spacing-xxs) var(--spacing-xs);
        line-height: 1;
    }

    .combobox-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        right: 0;
        margin-top: var(--spacing-xxs);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        box-shadow: var(--shadow-md);
        max-height: 200px;
        overflow-y: auto;
        z-index: var(--z-sticky);
    }

    .combobox-option {
        padding: var(--spacing-xs) var(--spacing-md);
        cursor: default;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .combobox-option:hover,
    .combobox-option.highlighted {
        background: var(--color-bg-secondary);
    }

    .combobox-option.selected {
        background: var(--color-accent-subtle);
    }

    .combobox-empty {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }
</style>
