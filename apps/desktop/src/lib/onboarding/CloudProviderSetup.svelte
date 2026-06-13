<script lang="ts">
    import { onDestroy } from 'svelte'
    import IconCheck from '~icons/lucide/check'
    import {
        getCloudProvider,
        getProviderConfigs,
        setProviderConfig,
        getSetting,
        setSetting,
    } from '$lib/settings'
    import {
        checkAiConnection,
        getAiApiKey,
        saveAiApiKey,
        openExternalUrl,
    } from '$lib/tauri-commands'
    import SettingPasswordInput from '$lib/settings/components/SettingPasswordInput.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import Combobox, { type ComboboxItem } from '$lib/ui/Combobox.svelte'
    import { describeSecretError, type SecretErrorMessage } from '$lib/settings/sections/ai-secret-error'
    import { getAppLogger } from '$lib/logging/logger'

    /**
     * Per-provider tutorial in the onboarding wizard's step 2 right column.
     *
     * Numbered steps, each with a checkmark that flips on when the step is satisfied:
     *
     *   1. Sign up at <provider>: always ✓ (informational link, can't fail here)
     *   2. Create an API key: link to the provider's API-key console; always ✓
     *   3. Paste your API key: ✓ when `connectionStatus === 'connected'`
     *   4. Pick a model: ✓ when the user has picked a non-empty model
     *
     * The connection-check pipeline mirrors `AiCloudSection.svelte`: 1 s debounce on
     * key/base-URL change, calls `checkAiConnection(baseUrl, apiKey)`, surfaces the
     * model list through the shared `ui/Combobox`. Unlike the settings section, the
     * model list already loads on open here (a stored key triggers a check in
     * `loadApiKeyForProvider`), so there's no separate mount-trigger to add.
     *
     * Provider switching is owned by the parent (`StepAi.svelte`); on `providerId`
     * change we reload the per-provider state from the store + secret keychain. Keys
     * already typed for previous providers stay in the secret store so users can hop
     * back without re-entering.
     */

    interface Props {
        providerId: string
    }

    const { providerId }: Props = $props()
    const log = getAppLogger('onboarding-ai-setup')

    type ConnectionStatus =
        | 'idle'
        | 'checking'
        | 'connected'
        | 'connected-no-models'
        | 'auth-error'
        | 'connection-error'
        | 'error'

    let currentApiKey = $state('')
    let currentModel = $state('')
    let currentBaseUrl = $state('')
    let connectionStatus = $state<ConnectionStatus>('idle')
    let connectionError = $state<string | null>(null)
    let availableModels = $state<string[]>([])
    let secretError = $state<SecretErrorMessage | null>(null)

    const API_KEY_SAVE_DEBOUNCE_MS = 300
    const CONNECTION_CHECK_DEBOUNCE_MS = 1000

    let apiKeySaveTimer: ReturnType<typeof setTimeout> | null = null
    let connectionCheckTimer: ReturnType<typeof setTimeout> | null = null
    // Captured at schedule time so a switch-provider-mid-typing flushes against the
    // right keychain entry (mirrors `AiCloudSection.flushPendingApiKeySave`).
    let pendingApiKeySave: { providerId: string; value: string } | null = null
    // Latest in-flight providerId, so a slow keychain read for a stale provider can
    // be dropped after the user clicks another row.
    let activeProviderId = $state(providerId)

    // Reload state whenever the parent picks a different provider.
    $effect(() => {
        const id = providerId
        activeProviderId = id
        // Flush any in-flight typing to the OLD provider so we don't lose those chars
        // by overwriting `currentApiKey` below.
        flushPendingApiKeySave()
        resetConnectionState()
        loadFromStore(id)
        void loadApiKeyForProvider(id)
    })

    onDestroy(() => {
        flushPendingApiKeySave()
        if (connectionCheckTimer) clearTimeout(connectionCheckTimer)
    })

    function loadFromStore(id: string): void {
        const preset = getCloudProvider(id)
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const providerConfig = configs[id]

        currentModel = providerConfig?.model ?? preset?.defaultModel ?? ''
        currentBaseUrl =
            id === 'custom' || id === 'azure-openai'
                ? (providerConfig?.baseUrl ?? preset?.baseUrl ?? '')
                : (preset?.baseUrl ?? '')
        currentApiKey = ''
        secretError = null
    }

    async function loadApiKeyForProvider(id: string): Promise<void> {
        try {
            const fetched = await getAiApiKey(id)
            if (id !== activeProviderId) return
            currentApiKey = fetched
            // If we just loaded a stored key, trigger an immediate check (no debounce):
            // the user expects "I came back; tell me if my key still works."
            if (currentApiKey !== '' && hasCheckableConfig()) {
                void triggerConnectionCheck()
            }
        } catch (e) {
            if (id !== activeProviderId) return
            secretError = describeSecretError(e, 'read')
        }
    }

    function hasCheckableConfig(): boolean {
        const preset = getCloudProvider(activeProviderId)
        const requiresApiKey = preset?.requiresApiKey ?? false
        const baseUrl = resolvedBaseUrl()
        if (requiresApiKey && currentApiKey === '') return false
        return baseUrl !== ''
    }

    function resolvedBaseUrl(): string {
        const preset = getCloudProvider(activeProviderId)
        if (activeProviderId === 'custom' || activeProviderId === 'azure-openai') {
            return currentBaseUrl
        }
        return preset?.baseUrl ?? ''
    }

    function scheduleConnectionCheck(delayMs: number = CONNECTION_CHECK_DEBOUNCE_MS): void {
        if (connectionCheckTimer) clearTimeout(connectionCheckTimer)
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
        if (!hasCheckableConfig()) return

        const baseUrl = resolvedBaseUrl()
        const key = currentApiKey
        const idAtStart = activeProviderId
        connectionStatus = 'checking'
        connectionError = null
        // Keep the prior list during a refetch so the model combobox never blanks mid-check.

        try {
            const result = await checkAiConnection(baseUrl, key)
            // Drop the result if the user switched providers mid-flight.
            if (idAtStart !== activeProviderId) return
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
            if (idAtStart !== activeProviderId) return
            connectionStatus = 'error'
            connectionError = e instanceof Error ? e.message : 'Something went wrong'
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

    function handleApiKeyChange(value: string): void {
        currentApiKey = value
        secretError = null
        pendingApiKeySave = { providerId: activeProviderId, value }
        if (apiKeySaveTimer) clearTimeout(apiKeySaveTimer)
        apiKeySaveTimer = setTimeout(() => {
            const pending = pendingApiKeySave
            apiKeySaveTimer = null
            pendingApiKeySave = null
            if (pending) void persistApiKey(pending.providerId, pending.value)
        }, API_KEY_SAVE_DEBOUNCE_MS)
    }

    function flushPendingApiKeySave(): void {
        if (!apiKeySaveTimer || !pendingApiKeySave) return
        clearTimeout(apiKeySaveTimer)
        const pending = pendingApiKeySave
        apiKeySaveTimer = null
        pendingApiKeySave = null
        void persistApiKey(pending.providerId, pending.value)
    }

    async function persistApiKey(id: string, value: string): Promise<void> {
        try {
            await saveAiApiKey(id, value)
        } catch (e) {
            secretError = describeSecretError(e, 'save')
            log.warn("Couldn't save AI API key from wizard for provider {provider}: {error}", {
                provider: id,
                error: e,
            })
            return
        }
        // Only check if we're still on this provider.
        if (id !== activeProviderId) return
        scheduleConnectionCheck()
    }

    function saveModel(value: string): void {
        currentModel = value
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const existing = configs[activeProviderId] ?? { model: '' }
        existing.model = value
        const updated = setProviderConfig(configsJson, activeProviderId, existing)
        setSetting('ai.cloudProviderConfigs', updated)
    }

    function saveBaseUrl(value: string): void {
        currentBaseUrl = value
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const existing = configs[activeProviderId] ?? { model: currentModel }
        existing.baseUrl = value
        const updated = setProviderConfig(configsJson, activeProviderId, existing)
        setSetting('ai.cloudProviderConfigs', updated)
        scheduleConnectionCheck()
    }

    function openProviderUrl(url: string): void {
        if (!url) return
        void openExternalUrl(url).catch((error: unknown) => {
            log.warn('openExternalUrl({url}) failed: {error}', { url, error })
        })
    }

    // Derived view state.
    const preset = $derived(getCloudProvider(activeProviderId))
    const showEditableBaseUrl = $derived(
        activeProviderId === 'custom' || activeProviderId === 'azure-openai',
    )
    const requiresApiKey = $derived(preset?.requiresApiKey ?? false)
    const apiKeyChecked = $derived(connectionStatus === 'connected' || connectionStatus === 'connected-no-models')
    const modelChecked = $derived(currentModel.trim() !== '')
    const modelComboboxItems = $derived<ComboboxItem[]>(availableModels.map((m) => ({ value: m, label: m })))
    const modelPlaceholder = $derived(preset?.defaultModel ? `Example: ${preset.defaultModel}` : 'Model name')

    // Per-provider sign-up and API-key console URLs. Kept inline because they're tied
    // to provider names a registry would just mirror; one source of truth per row.
    const providerLinksById: Record<string, { signup: string; apiKeys: string }> = {
        openai: { signup: 'https://platform.openai.com/signup', apiKeys: 'https://platform.openai.com/api-keys' },
        anthropic: {
            signup: 'https://platform.claude.com/login',
            apiKeys: 'https://platform.claude.com/settings/keys',
        },
        'google-gemini': {
            signup: 'https://aistudio.google.com/',
            apiKeys: 'https://aistudio.google.com/app/apikey',
        },
        groq: { signup: 'https://console.groq.com/login', apiKeys: 'https://console.groq.com/keys' },
        'together-ai': {
            signup: 'https://api.together.xyz/',
            apiKeys: 'https://api.together.xyz/settings/api-keys',
        },
        'fireworks-ai': {
            signup: 'https://app.fireworks.ai/login',
            apiKeys: 'https://app.fireworks.ai/settings/users/api-keys',
        },
        mistral: { signup: 'https://console.mistral.ai/', apiKeys: 'https://console.mistral.ai/api-keys/' },
        openrouter: { signup: 'https://openrouter.ai/', apiKeys: 'https://openrouter.ai/keys' },
        deepseek: { signup: 'https://platform.deepseek.com/', apiKeys: 'https://platform.deepseek.com/api_keys' },
        xai: { signup: 'https://console.x.ai/', apiKeys: 'https://console.x.ai/' },
        perplexity: {
            signup: 'https://www.perplexity.ai/settings/api',
            apiKeys: 'https://www.perplexity.ai/settings/api',
        },
        'azure-openai': {
            signup: 'https://azure.microsoft.com/en-us/products/ai-services/openai-service',
            apiKeys: 'https://portal.azure.com/',
        },
        ollama: { signup: 'https://ollama.com/download', apiKeys: 'https://ollama.com/' },
        'lm-studio': { signup: 'https://lmstudio.ai/', apiKeys: 'https://lmstudio.ai/docs/local-server' },
        custom: { signup: '', apiKeys: '' },
    }

    const links = $derived(providerLinksById[activeProviderId] ?? { signup: '', apiKeys: '' })
    const apiKeyPlaceholder = $derived(
        activeProviderId === 'openai'
            ? 'Example: sk-abc123...'
            : activeProviderId === 'anthropic'
              ? 'Example: sk-ant-abc123...'
              : 'API key',
    )
</script>

<div class="setup-panel" data-provider-id={activeProviderId}>
    {#if preset}
        <header class="provider-header">
            <h3 class="provider-title">Set up {preset.name}</h3>
            {#if preset.description}
                <p class="provider-description">{preset.description}</p>
            {/if}
        </header>

        <ol class="setup-steps">
            {#if links.signup}
                <li class="setup-step done">
                    <span class="step-marker" aria-hidden="true">
                        <IconCheck width="14" height="14" />
                    </span>
                    <div class="step-body">
                        <span class="step-label">
                            Sign up at
                            <LinkButton
                                href={links.signup}
                                target="_blank"
                                rel="noopener noreferrer"
                                onclick={(event: MouseEvent) => {
                                    event.preventDefault()
                                    openProviderUrl(links.signup)
                                }}
                            >
                                {preset.name}
                            </LinkButton>
                            (if you don't have an account)
                        </span>
                    </div>
                </li>
            {/if}

            {#if requiresApiKey && links.apiKeys}
                <li class="setup-step done">
                    <span class="step-marker" aria-hidden="true">
                        <IconCheck width="14" height="14" />
                    </span>
                    <div class="step-body">
                        <span class="step-label">
                            Create an API key
                            <LinkButton
                                href={links.apiKeys}
                                target="_blank"
                                rel="noopener noreferrer"
                                onclick={(event: MouseEvent) => {
                                    event.preventDefault()
                                    openProviderUrl(links.apiKeys)
                                }}
                            >
                                here
                            </LinkButton>
                        </span>
                    </div>
                </li>
            {/if}

            {#if showEditableBaseUrl}
                <li class="setup-step" class:done={currentBaseUrl.trim() !== ''}>
                    <span class="step-marker" aria-hidden="true">
                        {#if currentBaseUrl.trim() !== ''}
                            <IconCheck width="14" height="14" />
                        {/if}
                    </span>
                    <div class="step-body">
                        <label class="step-label" for="onboarding-cloud-base-url">Endpoint URL</label>
                        <input
                            id="onboarding-cloud-base-url"
                            class="text-input"
                            type="text"
                            value={currentBaseUrl}
                            oninput={(event: Event) => {
                                const target = event.currentTarget as HTMLInputElement
                                saveBaseUrl(target.value)
                            }}
                            placeholder="Example: https://api.example.com/v1"
                            autocomplete="off"
                            spellcheck="false"
                        />
                    </div>
                </li>
            {/if}

            {#if requiresApiKey}
                <li class="setup-step" class:done={apiKeyChecked}>
                    <span class="step-marker" aria-hidden="true">
                        {#if apiKeyChecked}
                            <IconCheck width="14" height="14" />
                        {/if}
                    </span>
                    <div class="step-body">
                        <label class="step-label" for="onboarding-cloud-api-key">Paste your API key</label>
                        <SettingPasswordInput
                            id="ai.cloudProviderConfigs"
                            placeholder={apiKeyPlaceholder}
                            ariaLabel="API key"
                            value={currentApiKey}
                            onchange={handleApiKeyChange}
                        />
                        {#if secretError}
                            <p class="status status-error" role="alert">{secretError.title}</p>
                        {:else if connectionStatus === 'checking'}
                            <p class="status status-checking">Checking your key…</p>
                        {:else if connectionStatus === 'auth-error'}
                            <p class="status status-error">{connectionError ?? "That key didn't work"}</p>
                        {:else if connectionStatus === 'connection-error'}
                            <p class="status status-error">
                                {connectionError ?? "Can't reach the service right now"}
                            </p>
                        {:else if connectionStatus === 'error'}
                            <p class="status status-error">{connectionError ?? 'Something went wrong'}</p>
                        {:else if apiKeyChecked}
                            <p class="status status-ok">Connected!</p>
                        {/if}
                    </div>
                </li>
            {/if}

            <li class="setup-step" class:done={modelChecked}>
                <span class="step-marker" aria-hidden="true">
                    {#if modelChecked}
                        <IconCheck width="14" height="14" />
                    {/if}
                </span>
                <div class="step-body">
                    <span class="step-label">Pick a model</span>
                    <Combobox
                        items={modelComboboxItems}
                        inputValue={currentModel}
                        onInputValueChange={saveModel}
                        loading={connectionStatus === 'checking'}
                        placeholder={modelPlaceholder}
                        ariaLabel="Model"
                    />
                </div>
            </li>
        </ol>
    {/if}
</div>

<style>
    .setup-panel {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
        min-height: 0;
    }

    .provider-header {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .provider-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .provider-description {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .setup-steps {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        counter-reset: setup-step;
    }

    .setup-step {
        display: grid;
        grid-template-columns: 24px 1fr;
        gap: var(--spacing-sm);
        align-items: start;
        counter-increment: setup-step;
    }

    .step-marker {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        border-radius: var(--radius-full);
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        line-height: 1;
    }

    .step-marker::before {
        content: counter(setup-step);
    }

    .setup-step.done .step-marker {
        /* Tinted bg + check icon in the allow color. Keep contrast over neutral text
           bg primary; the icon is decorative (the step status is also conveyed by the
           bolder body text), so we don't need a 4.5:1 token-pair. */
        background: transparent;
        color: var(--color-allow);
        border: 1px solid var(--color-allow);
    }

    .setup-step.done .step-marker::before {
        content: '';
    }

    .step-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .step-label {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        line-height: 1.4;
    }

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

    .status {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .status-checking {
        color: var(--color-text-secondary);
    }

    .status-ok {
        color: var(--color-allow);
    }

    .status-error {
        color: var(--color-error-text);
    }
</style>
