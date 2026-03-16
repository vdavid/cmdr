<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { listen } from '@tauri-apps/api/event'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingPasswordInput from '../components/SettingPasswordInput.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        type AiProvider,
        getCloudProvider,
        getProviderConfigs,
        setProviderConfig,
        resolveCloudConfig,
        cloudProviderPresets,
    } from '$lib/settings'
    import {
        getAiRuntimeStatus,
        configureAi,
        stopAiServer,
        startAiServer,
        startAiDownload,
        cancelAiDownload,
        uninstallAi,
        checkAiConnection,
        formatBytes,
        getSystemMemoryInfo,
        type AiRuntimeStatus,
        type AiDownloadProgress,
        type SystemMemoryInfo,
    } from '$lib/tauri-commands'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { computeGaugeSegments } from './ram-gauge-utils'
    import { getAppLogger } from '$lib/logging/logger'

    interface Props {
        searchQuery?: string
    }

    const { searchQuery = '' }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))
    const logger = getAppLogger('ai-settings')

    // Dynamic state from backend
    let status = $state<AiRuntimeStatus | null>(null)
    let isLoading = $state(true)
    let showDeleteConfirm = $state(false)
    let isDeleting = $state(false)
    let downloadProgress = $state<AiDownloadProgress | null>(null)
    let isRestarting = $state(false)

    // Multi-step install tracking
    type InstallStep = 'extracting' | 'downloading' | 'verifying' | 'starting' | null
    let installStep = $state<InstallStep>(null)
    let downloadCancelledByUser = $state(false)

    // Track current provider and context size for conditional rendering and memory estimate
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    let pendingContextSize = $state(Number(getSetting('ai.localContextSize')))

    // The context size the server is actually running with (set from backend status on mount,
    // updated after successful Apply)
    let activeContextSize = $state(Number(getSetting('ai.localContextSize')))

    // System memory info for the RAM gauge
    let systemMemory = $state<SystemMemoryInfo | null>(null)
    let memoryPollInterval: ReturnType<typeof setInterval> | null = null

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

    onMount(async () => {
        try {
            status = await getAiRuntimeStatus()
            // If server is running, activeContextSize is what the server started with.
            // The backend doesn't expose the running context size directly, so we use the
            // current setting as the best approximation on mount.
            if (status.serverRunning) {
                activeContextSize = pendingContextSize
            }
        } catch (e) {
            logger.error("Couldn't load AI status: {error}", { error: e })
        } finally {
            isLoading = false
        }

        // Migrate old settings to new per-provider config if needed
        migrateOldSettings()

        // Load current cloud provider config into local state
        loadCloudProviderConfig(cloudProviderId)

        // Subscribe to provider changes
        const unsubProvider = onSpecificSettingChange('ai.provider', (_id, newValue) => {
            const oldProvider = provider
            provider = newValue
            void handleProviderChange(oldProvider, provider)
        })
        unlistenFns.push(unsubProvider)

        // Subscribe to context size changes (update pending, no auto-restart)
        const unsubCtx = onSpecificSettingChange('ai.localContextSize', (_id, newValue) => {
            pendingContextSize = Number(newValue)
        })
        unlistenFns.push(unsubCtx)

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

        // Listen for backend events
        const unlistenReady = await listen('ai-server-ready', () => {
            isRestarting = false
            activeContextSize = pendingContextSize
            void refreshStatus()
        })
        unlistenFns.push(unlistenReady)

        const unlistenExtracting = await listen('ai-extracting', () => {
            installStep = 'extracting'
        })
        unlistenFns.push(unlistenExtracting)

        const unlistenProgress = await listen<AiDownloadProgress>('ai-download-progress', (event) => {
            downloadProgress = event.payload
            if (installStep !== 'downloading') {
                installStep = 'downloading'
            }
        })
        unlistenFns.push(unlistenProgress)

        const unlistenVerifying = await listen('ai-verifying', () => {
            installStep = 'verifying'
        })
        unlistenFns.push(unlistenVerifying)

        const unlistenInstalling = await listen('ai-installing', () => {
            installStep = 'starting'
        })
        unlistenFns.push(unlistenInstalling)

        const unlistenInstallComplete = await listen('ai-install-complete', () => {
            installStep = null
            downloadProgress = null
            void refreshStatus()
        })
        unlistenFns.push(unlistenInstallComplete)

        const unlistenStarting = await listen('ai-starting', () => {
            void refreshStatus()
        })
        unlistenFns.push(unlistenStarting)

        // Poll system memory every 5 seconds
        await pollSystemMemory()
        memoryPollInterval = setInterval(() => void pollSystemMemory(), 5000)
    })

    onDestroy(() => {
        for (const fn of unlistenFns) {
            fn()
        }
        if (memoryPollInterval) {
            clearInterval(memoryPollInterval)
        }
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
        }
    })

    async function pollSystemMemory(): Promise<void> {
        try {
            systemMemory = await getSystemMemoryInfo()
        } catch (e) {
            logger.error("Couldn't get system memory info: {error}", { error: e })
        }
    }

    async function refreshStatus(): Promise<void> {
        try {
            status = await getAiRuntimeStatus()
        } catch (e) {
            logger.error("Couldn't refresh AI status: {error}", { error: e })
        }
    }

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

    async function handleProviderChange(oldProvider: AiProvider, newProvider: AiProvider): Promise<void> {
        // Stop server when leaving local
        if (oldProvider === 'local' && newProvider !== 'local') {
            try {
                await stopAiServer()
            } catch (e) {
                logger.error("Couldn't stop AI server: {error}", { error: e })
            }
        }

        // Push config to backend (which handles starting server if needed)
        await pushConfigToBackend()
        await refreshStatus()
    }

    async function performContextRestart(): Promise<void> {
        isRestarting = true
        try {
            await stopAiServer()
            const ctxSize = Number(getSetting('ai.localContextSize'))
            await startAiServer(ctxSize)
        } catch (e) {
            logger.error("Couldn't restart AI server: {error}", { error: e })
            isRestarting = false
        }
        await refreshStatus()
    }

    async function handleApplyContextSize(): Promise<void> {
        await performContextRestart()
    }

    async function handleStartServer(): Promise<void> {
        try {
            const ctxSize = Number(getSetting('ai.localContextSize'))
            await startAiServer(ctxSize)
            activeContextSize = ctxSize
            await refreshStatus()
        } catch (e) {
            logger.error("Couldn't start AI server: {error}", { error: e })
        }
    }

    async function handleStopServer(): Promise<void> {
        try {
            await stopAiServer()
            await refreshStatus()
        } catch (e) {
            logger.error("Couldn't stop AI server: {error}", { error: e })
        }
    }

    async function handleDownloadModel(): Promise<void> {
        downloadCancelledByUser = false
        installStep = 'extracting'
        downloadProgress = { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 }
        try {
            await startAiDownload()
        } catch (e) {
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- set by handleCancelDownload() during the await
            if (downloadCancelledByUser) {
                logger.info('AI download cancelled by user')
            } else {
                logger.error("Couldn't start AI download: {error}", { error: e })
            }
            downloadProgress = null
            installStep = null
        }
        await refreshStatus()
    }

    async function handleCancelDownload(): Promise<void> {
        downloadCancelledByUser = true
        try {
            await cancelAiDownload()
        } catch (e) {
            logger.error("Couldn't cancel AI download: {error}", { error: e })
        }
        installStep = null
        downloadProgress = null
        await refreshStatus()
    }

    async function handleDeleteModel(): Promise<void> {
        isDeleting = true
        try {
            await uninstallAi()
        } catch (e) {
            logger.error("Couldn't delete AI model: {error}", { error: e })
        }
        isDeleting = false
        showDeleteConfirm = false
        await refreshStatus()
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
    const localAiSupported = $derived(status?.localAiSupported ?? true)
    const modelInstalled = $derived(status?.modelInstalled ?? false)
    const serverRunning = $derived(status?.serverRunning ?? false)
    const serverStarting = $derived(status?.serverStarting ?? false)

    // Current cloud provider preset
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

    const providerTooltips: Record<string, string> = {
        off: 'AI features are turned off. Cmdr works fully without AI \u2014 suggestions and smart features are simply hidden.',
        'openai-compatible':
            "Bring your own API key for fast, high-quality AI. Works with OpenAI, Groq, Together AI, Azure OpenAI, Anthropic (via proxy), or any local server you're running (Ollama, LM Studio, etc.). Requires an internet connection (unless using a local server). No disk space or memory used by Cmdr.",
        local: 'Runs a small language model entirely on your device. Maximum privacy \u2014 nothing leaves your computer. Works offline. Uses ~2 GB disk space and ~400 MB memory (varies with context size). Requires Apple Silicon (M1+).',
        'local-disabled': 'Local AI requires Apple Silicon (M1 or later). Use OpenAI-compatible instead.',
    }

    function getProviderTooltip(value: string): string {
        if (value === 'local' && !localAiSupported) return providerTooltips['local-disabled']
        return providerTooltips[value] ?? ''
    }

    function handleProviderSelect(value: AiProvider): void {
        if (value === 'local' && !localAiSupported) return
        if (value === provider) return
        resetConnectionState()
        setSetting('ai.provider', value)
    }

    // Memory estimate
    const kvBytesPerToken = $derived(status?.kvBytesPerToken ?? 106496)
    const baseOverheadBytes = $derived(status?.baseOverheadBytes ?? 3500000000)

    // Current AI memory (what the server is actually using now)
    const currentAiMemoryBytes = $derived(
        serverRunning || serverStarting ? kvBytesPerToken * activeContextSize + baseOverheadBytes : 0,
    )

    // Projected AI memory (what the pending context size would use)
    const projectedAiMemoryBytes = $derived(kvBytesPerToken * pendingContextSize + baseOverheadBytes)
    const projectedMemoryFormatted = $derived(formatMemoryEstimate(projectedAiMemoryBytes))

    // Whether the Apply button should be visible
    const showApplyButton = $derived(pendingContextSize !== activeContextSize && serverRunning && !isRestarting)

    // RAM gauge segments (percentages of total RAM)
    const gaugeSegments = $derived(
        systemMemory ? computeGaugeSegments(systemMemory, currentAiMemoryBytes, projectedAiMemoryBytes) : null,
    )

    // Warning state based on projected usage
    const warningLevel = $derived.by((): 'none' | 'caution' | 'danger' => {
        if (!gaugeSegments) return 'none'
        if (gaugeSegments.totalProjectedUsageRatio > 0.9) return 'danger'
        if (gaugeSegments.totalProjectedUsageRatio > 0.7) return 'caution'
        return 'none'
    })

    const warningTooltip = $derived.by(() => {
        if (warningLevel === 'danger')
            return 'This exceeds your available memory. Your system may slow down significantly.'
        if (warningLevel === 'caution') return 'This uses most of your available memory. Other apps may slow down.'
        return ''
    })

    // Server status text
    const serverStatusText = $derived.by(() => {
        if (isRestarting) return 'Restarting...'
        if (serverStarting) return 'Starting...'
        if (serverRunning) return 'Running'
        return 'Stopped'
    })

    const serverStatusDetail = $derived.by(() => {
        if (!serverRunning || !status?.pid) return ''
        const parts = [`PID ${String(status.pid)}`]
        if (status.port) parts.push(`port ${String(status.port)}`)
        return parts.join(' \u00b7 ')
    })

    // Download progress
    const downloadPercent = $derived(
        downloadProgress && downloadProgress.totalBytes > 0
            ? Math.round((downloadProgress.bytesDownloaded / downloadProgress.totalBytes) * 100)
            : 0,
    )
    const downloadProgressText = $derived.by(() => {
        if (!downloadProgress) return ''
        if (downloadProgress.totalBytes === 0) return 'Starting download...'
        const downloaded = formatBytes(downloadProgress.bytesDownloaded)
        const total = formatBytes(downloadProgress.totalBytes)
        const speed = formatBytes(downloadProgress.speed)
        const eta = formatEta(downloadProgress.etaSeconds)
        const parts = [`${String(downloadPercent)}%`, `${downloaded} / ${total}`, `${speed}/s`]
        if (eta) parts.push(eta)
        return parts.join(' \u00b7 ')
    })

    // Install step display
    const installStepLabel = $derived.by(() => {
        switch (installStep) {
            case 'extracting':
                return 'Step 1 of 4: Extracting runtime...'
            case 'downloading':
                return 'Step 2 of 4: Downloading model...'
            case 'verifying':
                return 'Step 3 of 4: Verifying download...'
            case 'starting':
                return 'Step 4 of 4: Starting server...'
            default:
                return ''
        }
    })

    // Whether actions should be disabled (during starting/restarting)
    const actionsDisabled = $derived(serverStarting || isRestarting)

    function formatEta(seconds: number): string {
        if (seconds <= 0) return ''
        if (seconds < 60) return `~${String(Math.ceil(seconds))} sec left`
        if (seconds < 3600) return `~${String(Math.ceil(seconds / 60))} min left`
        return `~${String(Math.round(seconds / 3600))} hr left`
    }

    function formatMemoryEstimate(bytes: number): string {
        const gb = bytes / (1024 * 1024 * 1024)
        return `~${gb.toFixed(1)} GB`
    }

    function formatMemoryGb(bytes: number): string {
        const gb = bytes / (1024 * 1024 * 1024)
        return `${gb.toFixed(1)} GB`
    }
</script>

<SettingsSection title="AI">
    {#if isLoading}
        <p class="loading-text">Loading...</p>
    {:else}
        <!-- Provider toggle with per-option tooltips -->
        {#if shouldShow('ai.provider')}
            <SettingRow
                id="ai.provider"
                label="Provider"
                description="Choose how AI features are powered."
                {searchQuery}
            >
                <div class="provider-toggle" role="radiogroup" aria-label="AI provider">
                    <button
                        class="provider-option"
                        class:selected={provider === 'off'}
                        onclick={() => {
                            handleProviderSelect('off')
                        }}
                        use:tooltip={getProviderTooltip('off')}
                        role="radio"
                        aria-checked={provider === 'off'}
                    >
                        Off
                    </button>
                    <button
                        class="provider-option"
                        class:selected={provider === 'openai-compatible'}
                        onclick={() => {
                            handleProviderSelect('openai-compatible')
                        }}
                        use:tooltip={getProviderTooltip('openai-compatible')}
                        role="radio"
                        aria-checked={provider === 'openai-compatible'}
                    >
                        Cloud / API
                    </button>
                    <button
                        class="provider-option"
                        class:selected={provider === 'local'}
                        disabled={!localAiSupported}
                        onclick={() => {
                            handleProviderSelect('local')
                        }}
                        use:tooltip={getProviderTooltip('local')}
                        role="radio"
                        aria-checked={provider === 'local'}
                    >
                        Local LLM
                    </button>
                </div>
            </SettingRow>
        {/if}

        <!-- Cloud / API section -->
        {#if provider === 'openai-compatible'}
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
        {/if}

        <!-- Local LLM section -->
        {#if provider === 'local'}
            <!-- Status card -->
            <div class="status-card">
                {#if installStep !== null}
                    <div class="install-step-label">{installStepLabel}</div>
                    {#if installStep === 'downloading'}
                        <div class="progress-bar-container">
                            <div class="progress-bar-fill" style="width: {String(downloadPercent)}%"></div>
                        </div>
                        <span class="progress-text">{downloadProgressText}</span>
                    {/if}
                {:else if modelInstalled}
                    <div class="status-row">
                        <span class="status-label">Model</span>
                        <span class="status-value"
                            >{status?.modelName ?? 'Unknown'} ({status?.modelSizeFormatted ?? '?'})</span
                        >
                    </div>
                    <div class="status-row">
                        <span class="status-label">Server</span>
                        <span
                            class="status-value"
                            class:status-running={serverRunning && !isRestarting && !serverStarting}
                            class:status-stopped={!serverRunning && !serverStarting && !isRestarting}
                        >
                            {serverStatusText}{#if serverStatusDetail}
                                <span class="status-detail">&middot; {serverStatusDetail}</span>{/if}
                        </span>
                    </div>
                {:else}
                    <p class="not-installed-text">
                        Not installed. The local model ({status?.modelName ?? 'Ministral 3B'}, {status?.modelSizeFormatted ??
                            '2.0 GB'}) runs entirely on your device for maximum privacy. Requires Apple Silicon.
                    </p>
                {/if}
            </div>

            <!-- Context window (only when installed) -->
            {#if modelInstalled && shouldShow('ai.localContextSize')}
                <SettingRow
                    id="ai.localContextSize"
                    label="Context window"
                    description="Number of tokens the local model can process at once. Larger values use more memory."
                    split
                    {searchQuery}
                >
                    <div class="context-size-controls">
                        <SettingSelect id="ai.localContextSize" />
                        {#if showApplyButton}
                            <div class="apply-wrapper">
                                {#if warningLevel === 'caution'}
                                    <span
                                        class="warning-icon warning-caution"
                                        use:tooltip={warningTooltip}
                                        aria-label="Memory warning"
                                    >
                                        &#x26A0;
                                    </span>
                                {:else if warningLevel === 'danger'}
                                    <span
                                        class="warning-icon warning-danger"
                                        use:tooltip={warningTooltip}
                                        aria-label="Memory warning"
                                    >
                                        &#x26A0;
                                    </span>
                                {/if}
                                <Button
                                    variant="primary"
                                    size="mini"
                                    disabled={actionsDisabled}
                                    onclick={() => void handleApplyContextSize()}
                                >
                                    Apply
                                </Button>
                            </div>
                        {/if}
                    </div>
                </SettingRow>

                <!-- RAM gauge -->
                {#if systemMemory && systemMemory.totalBytes > 0 && gaugeSegments}
                    <div class="ram-gauge-container" aria-label="Memory usage gauge">
                        <div class="ram-gauge-bar">
                            <div
                                class="ram-segment ram-system"
                                style="width: {gaugeSegments.systemPercent.toFixed(2)}%"
                            ></div>
                            <div
                                class="ram-segment ram-other-apps"
                                style="width: {gaugeSegments.otherAppsPercent.toFixed(2)}%"
                            ></div>
                            <div
                                class="ram-segment ram-current-ai"
                                style="width: {gaugeSegments.retainedAiPercent.toFixed(2)}%"
                            ></div>
                            {#if gaugeSegments.addedPercent > 0}
                                <div
                                    class="ram-segment ram-projected"
                                    style="width: {gaugeSegments.addedPercent.toFixed(2)}%"
                                ></div>
                            {/if}
                            {#if gaugeSegments.freedPercent > 0}
                                <div
                                    class="ram-segment ram-freed"
                                    style="width: {gaugeSegments.freedPercent.toFixed(2)}%"
                                ></div>
                            {/if}
                        </div>
                        <div class="ram-legend">
                            <span class="ram-legend-item"
                                ><span class="ram-legend-swatch ram-system"></span>System {formatMemoryGb(
                                    gaugeSegments.systemBytes,
                                )}</span
                            >
                            <span class="ram-legend-item"
                                ><span class="ram-legend-swatch ram-other-apps"></span>Apps {formatMemoryGb(
                                    gaugeSegments.otherAppsBytes,
                                )}</span
                            >
                            <span class="ram-legend-item"
                                ><span class="ram-legend-swatch ram-current-ai"></span>Cmdr AI {projectedMemoryFormatted}</span
                            >
                            {#if gaugeSegments.addedPercent > 0}
                                <span class="ram-legend-item"
                                    ><span class="ram-legend-swatch ram-projected"></span>Projected</span
                                >
                            {/if}
                            {#if gaugeSegments.freedPercent > 0}
                                <span class="ram-legend-item"
                                    ><span class="ram-legend-swatch ram-freed"></span>Freed</span
                                >
                            {/if}
                            <span class="ram-legend-item"
                                ><span class="ram-legend-swatch ram-free-space"></span>Free {formatMemoryGb(
                                    gaugeSegments.freeBytes,
                                )}</span
                            >
                        </div>
                    </div>
                {/if}
            {/if}

            <!-- Actions -->
            <div class="actions">
                {#if installStep === 'extracting' || installStep === 'downloading'}
                    <Button variant="secondary" onclick={() => void handleCancelDownload()}>Cancel</Button>
                {:else if installStep !== null}
                    <!-- Verifying/starting: can't cancel after download completes -->
                {:else if modelInstalled}
                    {#if serverRunning}
                        <Button variant="secondary" disabled={actionsDisabled} onclick={() => void handleStopServer()}
                            >Stop server</Button
                        >
                    {:else}
                        <Button variant="secondary" disabled={actionsDisabled} onclick={() => void handleStartServer()}
                            >Start server</Button
                        >
                    {/if}
                    <Button variant="danger" disabled={actionsDisabled} onclick={() => (showDeleteConfirm = true)}
                        >Delete model</Button
                    >
                {:else}
                    <Button variant="secondary" onclick={() => void handleDownloadModel()}>Download model</Button>
                {/if}
            </div>
        {/if}
    {/if}
</SettingsSection>

<!-- Delete model confirmation dialog -->
{#if showDeleteConfirm}
    <ModalDialog
        titleId="delete-ai-model-title"
        dialogId="delete-ai-model"
        role="alertdialog"
        onclose={() => {
            if (!isDeleting) showDeleteConfirm = false
        }}
        containerStyle="width: 400px"
        onkeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter' && !isDeleting) {
                void handleDeleteModel()
            }
        }}
    >
        {#snippet title()}{isDeleting ? 'Deleting model...' : 'Delete AI model?'}{/snippet}
        <div class="confirm-body">
            {#if isDeleting}
                <div class="deleting-status">
                    <span class="status-spinner">&#x27F3;</span>
                    <span>Stopping server and removing files...</span>
                </div>
            {:else}
                <p class="confirm-message">
                    This frees up {status?.modelSizeFormatted ?? '2.0 GB'} of disk space. You'll need to re-download it to
                    use local AI again.
                </p>
            {/if}
            <div class="confirm-buttons">
                <Button variant="secondary" disabled={isDeleting} onclick={() => (showDeleteConfirm = false)}
                    >Cancel</Button
                >
                <Button variant="danger" disabled={isDeleting} onclick={() => void handleDeleteModel()}>
                    {isDeleting ? 'Deleting...' : 'Delete'}
                </Button>
            </div>
        </div>
    </ModalDialog>
{/if}

<style>
    .loading-text {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }

    /* Provider toggle (custom, matches SettingToggleGroup styling) */
    .provider-toggle {
        display: inline-flex;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .provider-option {
        padding: var(--spacing-xs) var(--spacing-md);
        border: none;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
        transition: all var(--transition-base);
        border-right: 1px solid var(--color-border);
    }

    .provider-option:last-child {
        border-right: none;
    }

    .provider-option.selected {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .provider-option.selected:hover {
        background: var(--color-accent-hover);
    }

    .provider-option:disabled {
        cursor: not-allowed;
        opacity: 0.5;
    }

    .provider-option:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
        z-index: 1;
    }

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

    /* Status card */
    .status-card {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-xs) var(--spacing-lg);
        margin-bottom: var(--spacing-lg);
    }

    .status-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--spacing-xl);
        padding: var(--spacing-sm) 0;
    }

    .status-row:not(:last-child) {
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .status-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .status-value {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        font-weight: 500;
        text-align: right;
    }

    .status-detail {
        color: var(--color-text-secondary);
        font-weight: 400;
    }

    .status-running {
        color: var(--color-allow);
    }

    .status-stopped {
        color: var(--color-text-tertiary);
    }

    .not-installed-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
        margin: var(--spacing-xs) 0;
    }

    /* Install step */
    .install-step-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        padding: var(--spacing-sm) 0 var(--spacing-xs);
    }

    /* Progress bar */
    .progress-bar-container {
        width: 100%;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        overflow: hidden;
        margin: var(--spacing-xs) 0;
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width var(--transition-slow);
    }

    .progress-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    /* Context size controls with Apply button */
    .context-size-controls {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .apply-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .warning-icon {
        font-size: var(--font-size-md);
        line-height: 1;
        cursor: default;
    }

    .warning-caution {
        color: var(--color-warning);
    }

    .warning-danger {
        color: var(--color-error);
    }

    /* RAM gauge */
    .ram-gauge-container {
        margin: var(--spacing-xs) 0 0;
    }

    .ram-gauge-bar {
        display: flex;
        width: 100%;
        height: 5px;
        border-radius: var(--radius-sm);
        overflow: hidden;
        background: var(--color-bg-secondary);
    }

    .ram-segment {
        height: 100%;
        flex-shrink: 0;
        min-width: 0;
    }

    .ram-system {
        background: var(--color-border);
    }

    .ram-other-apps {
        background: var(--color-text-tertiary);
    }

    .ram-current-ai {
        background: var(--color-accent);
    }

    .ram-projected {
        background: var(--color-accent);
        opacity: 0.5;
    }

    .ram-freed {
        background: var(--color-allow);
        opacity: 0.5;
    }

    .ram-legend {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm) var(--spacing-md);
        margin-top: var(--spacing-xs);
    }

    .ram-legend-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .ram-legend-swatch {
        display: inline-block;
        width: 8px;
        height: 8px;
        border-radius: var(--radius-xs);
    }

    .ram-free-space {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
    }

    /* Actions */
    .actions {
        display: flex;
        gap: var(--spacing-md);
        margin-top: var(--spacing-sm);
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

    /* Delete confirmation dialog */
    .confirm-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .confirm-message {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        text-align: center;
        line-height: 1.4;
    }

    .confirm-buttons {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }

    .deleting-status {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
