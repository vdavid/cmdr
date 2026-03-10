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
    import { getSetting, setSetting, onSpecificSettingChange, type AiProvider } from '$lib/settings'
    import {
        getAiRuntimeStatus,
        configureAi,
        stopAiServer,
        startAiServer,
        startAiDownload,
        cancelAiDownload,
        uninstallAi,
        formatBytes,
        type AiRuntimeStatus,
        type AiDownloadProgress,
    } from '$lib/tauri-commands'
    import { createShouldShow } from '$lib/settings/settings-search'
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
    let downloadProgress = $state<AiDownloadProgress | null>(null)
    let isRestarting = $state(false)

    // Track current provider and context size for conditional rendering and memory estimate
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    let currentContextSize = $state(Number(getSetting('ai.localContextSize')))

    // Debounce timer for context size restart
    let contextRestartTimer: ReturnType<typeof setTimeout> | null = null

    // Event listeners cleanup
    const unlistenFns: Array<() => void> = []

    onMount(async () => {
        try {
            status = await getAiRuntimeStatus()
        } catch (e) {
            logger.error("Couldn't load AI status: {error}", { error: e })
        } finally {
            isLoading = false
        }

        // Subscribe to provider changes
        const unsubProvider = onSpecificSettingChange('ai.provider', (_id, newValue) => {
            const oldProvider = provider
            provider = newValue
            void handleProviderChange(oldProvider, provider)
        })
        unlistenFns.push(unsubProvider)

        // Subscribe to context size changes (debounced restart + memory estimate update)
        const unsubCtx = onSpecificSettingChange('ai.localContextSize', (_id, newValue) => {
            currentContextSize = Number(newValue)
            if (provider === 'local' && status?.modelInstalled && status.serverRunning) {
                scheduleContextRestart()
            }
        })
        unlistenFns.push(unsubCtx)

        // Subscribe to OpenAI config changes — push to backend
        for (const settingId of ['ai.openaiApiKey', 'ai.openaiBaseUrl', 'ai.openaiModel'] as const) {
            const unsub = onSpecificSettingChange(settingId, () => {
                void pushConfigToBackend()
            })
            unlistenFns.push(unsub)
        }

        // Listen for backend events
        const unlistenReady = await listen('ai-server-ready', () => {
            isRestarting = false
            void refreshStatus()
        })
        unlistenFns.push(unlistenReady)

        const unlistenProgress = await listen<AiDownloadProgress>('ai-download-progress', (event) => {
            downloadProgress = event.payload
        })
        unlistenFns.push(unlistenProgress)

        const unlistenInstallComplete = await listen('ai-install-complete', () => {
            downloadProgress = null
            void refreshStatus()
        })
        unlistenFns.push(unlistenInstallComplete)

        const unlistenStarting = await listen('ai-starting', () => {
            void refreshStatus()
        })
        unlistenFns.push(unlistenStarting)
    })

    onDestroy(() => {
        for (const fn of unlistenFns) {
            fn()
        }
        if (contextRestartTimer) {
            clearTimeout(contextRestartTimer)
        }
    })

    async function refreshStatus(): Promise<void> {
        try {
            status = await getAiRuntimeStatus()
        } catch (e) {
            logger.error("Couldn't refresh AI status: {error}", { error: e })
        }
    }

    async function pushConfigToBackend(): Promise<void> {
        try {
            await configureAi(
                getSetting('ai.provider'),
                Number(getSetting('ai.localContextSize')),
                getSetting('ai.openaiApiKey'),
                getSetting('ai.openaiBaseUrl'),
                getSetting('ai.openaiModel'),
            )
        } catch (e) {
            logger.error("Couldn't push AI config to backend: {error}", { error: e })
        }
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

    function scheduleContextRestart(): void {
        if (contextRestartTimer) {
            clearTimeout(contextRestartTimer)
        }
        contextRestartTimer = setTimeout(() => {
            contextRestartTimer = null
            void performContextRestart()
        }, 2000)
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

    async function handleStartServer(): Promise<void> {
        try {
            const ctxSize = Number(getSetting('ai.localContextSize'))
            await startAiServer(ctxSize)
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
        downloadProgress = { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 }
        try {
            await startAiDownload()
        } catch (e) {
            logger.error("Couldn't start AI download: {error}", { error: e })
            downloadProgress = null
        }
        await refreshStatus()
    }

    async function handleCancelDownload(): Promise<void> {
        try {
            await cancelAiDownload()
        } catch (e) {
            logger.error("Couldn't cancel AI download: {error}", { error: e })
        }
        downloadProgress = null
        await refreshStatus()
    }

    async function handleDeleteModel(): Promise<void> {
        showDeleteConfirm = false
        try {
            await uninstallAi()
        } catch (e) {
            logger.error("Couldn't delete AI model: {error}", { error: e })
        }
        await refreshStatus()
    }

    // Derived state
    const localAiSupported = $derived(status?.localAiSupported ?? true)
    const modelInstalled = $derived(status?.modelInstalled ?? false)
    const serverRunning = $derived(status?.serverRunning ?? false)
    const serverStarting = $derived(status?.serverStarting ?? false)
    const isDownloading = $derived(
        downloadProgress !== null && ((status?.downloadInProgress ?? false) || downloadProgress.totalBytes > 0),
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
        setSetting('ai.provider', value)
    }

    // Memory estimate
    const kvBytesPerToken = $derived(status?.kvBytesPerToken ?? 106496)
    const baseOverheadBytes = $derived(status?.baseOverheadBytes ?? 3500000000)
    const estimatedMemoryBytes = $derived(kvBytesPerToken * currentContextSize + baseOverheadBytes)
    const estimatedMemoryFormatted = $derived(formatMemoryEstimate(estimatedMemoryBytes))

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
        return `${String(downloadPercent)}% \u00b7 ${downloaded} / ${total} \u00b7 ${speed}/s`
    })

    // Whether actions should be disabled (during starting/restarting)
    const actionsDisabled = $derived(serverStarting || isRestarting)

    function formatMemoryEstimate(bytes: number): string {
        const gb = bytes / (1024 * 1024 * 1024)
        return `~${gb.toFixed(1)} GB`
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
                        OpenAI-compatible
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

        <!-- OpenAI-compatible section -->
        {#if provider === 'openai-compatible'}
            {#if shouldShow('ai.openaiApiKey')}
                <SettingRow
                    id="ai.openaiApiKey"
                    label="API key"
                    description="Your OpenAI-compatible API key."
                    {searchQuery}
                >
                    <SettingPasswordInput
                        id="ai.openaiApiKey"
                        placeholder="Example: sk-abc123..."
                        ariaLabel="API key"
                    />
                </SettingRow>
            {/if}

            {#if shouldShow('ai.openaiBaseUrl')}
                <SettingRow
                    id="ai.openaiBaseUrl"
                    label="Base URL"
                    description="API endpoint. Change this for Groq, Together AI, Azure OpenAI, or a local server."
                    {searchQuery}
                >
                    <input
                        class="text-input"
                        type="text"
                        value={getSetting('ai.openaiBaseUrl')}
                        oninput={(e: Event) => {
                            const target = e.target as HTMLInputElement
                            setSetting('ai.openaiBaseUrl', target.value)
                        }}
                        placeholder="Example: https://api.openai.com/v1"
                        aria-label="Base URL"
                        autocomplete="off"
                        spellcheck="false"
                    />
                </SettingRow>
            {/if}

            {#if shouldShow('ai.openaiModel')}
                <SettingRow
                    id="ai.openaiModel"
                    label="Model"
                    description="The model name to use for completions."
                    {searchQuery}
                >
                    <input
                        class="text-input"
                        type="text"
                        value={getSetting('ai.openaiModel')}
                        oninput={(e: Event) => {
                            const target = e.target as HTMLInputElement
                            setSetting('ai.openaiModel', target.value)
                        }}
                        placeholder="Example: gpt-4o-mini"
                        aria-label="Model"
                        autocomplete="off"
                        spellcheck="false"
                    />
                </SettingRow>
            {/if}
        {/if}

        <!-- Local LLM section -->
        {#if provider === 'local'}
            <!-- Status card -->
            <div class="status-card">
                {#if isDownloading}
                    <div class="status-row">
                        <span class="status-label">Downloading {status?.modelName ?? 'model'}...</span>
                    </div>
                    <div class="progress-bar-container">
                        <div class="progress-bar-fill" style="width: {String(downloadPercent)}%"></div>
                    </div>
                    <span class="progress-text">{downloadProgressText}</span>
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
                    {searchQuery}
                >
                    <SettingSelect id="ai.localContextSize" />
                </SettingRow>
                <p class="memory-estimate">Estimated memory use: {estimatedMemoryFormatted}</p>
            {/if}

            <!-- Actions -->
            <div class="actions">
                {#if isDownloading}
                    <Button variant="secondary" onclick={() => void handleCancelDownload()}>Cancel download</Button>
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
        onclose={() => (showDeleteConfirm = false)}
        containerStyle="width: 400px"
        onkeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter') {
                void handleDeleteModel()
            }
        }}
    >
        {#snippet title()}Delete AI model?{/snippet}
        <div class="confirm-body">
            <p class="confirm-message">
                This frees up {status?.modelSizeFormatted ?? '2.0 GB'} of disk space. You'll need to re-download it to use
                local AI again.
            </p>
            <div class="confirm-buttons">
                <Button variant="secondary" onclick={() => (showDeleteConfirm = false)}>Cancel</Button>
                <Button variant="danger" onclick={() => void handleDeleteModel()}>Delete</Button>
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

    /* Text input (same style as other setting inputs) */
    .text-input {
        min-width: 180px;
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

    /* Progress bar */
    .progress-bar-container {
        width: 100%;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: 2px;
        overflow: hidden;
        margin: var(--spacing-xs) 0;
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: 2px;
        transition: width var(--transition-slow);
    }

    .progress-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    /* Memory estimate */
    .memory-estimate {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: var(--spacing-xs) 0 0;
    }

    /* Actions */
    .actions {
        display: flex;
        gap: var(--spacing-md);
        margin-top: var(--spacing-sm);
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
</style>
