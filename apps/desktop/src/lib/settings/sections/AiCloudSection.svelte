<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingPasswordInput from '../components/SettingPasswordInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import Combobox, { type ComboboxItem } from '$lib/ui/Combobox.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        getCloudProvider,
        getProviderConfigs,
        setProviderConfig,
        cloudProviderPresets,
    } from '$lib/settings'
    import { checkAiConnection, getAiApiKey, saveAiApiKey } from '$lib/tauri-commands'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import { computeModelCacheKey, getCachedModels, setCachedModels } from '$lib/settings/ai-model-cache'
    import { getAppMode } from '$lib/app-mode'
    import { describeSecretError, type SecretErrorMessage } from './ai-secret-error'
    import { addToast, dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
        shouldShow: (id: string) => boolean
    }

    const { searchQuery, shouldShow }: Props = $props()

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

    // Secret store error (read or save failed). Shown inline above the connection status, and
    // re-emitted as a persistent toast so the user can act on it without re-opening Settings.
    // Reused across save/read attempts via a stable toast id, so we replace in place instead of
    // stacking duplicates.
    let secretError = $state<SecretErrorMessage | null>(null)
    const secretErrorToastId = 'ai-secret-store-error'

    // Debounce API key saves so manual typing doesn't fire one secret-store write per keystroke
    // (especially relevant on Linux where every Secret Service call is a D-Bus round trip).
    // Paste arrives as a single oninput so it sees no added latency. 300 ms is short enough that
    // the save feels instantaneous after the user pauses, and well under the connection-check
    // debounce (1000 ms) so the order is: type → save → check.
    const API_KEY_SAVE_DEBOUNCE_MS = 300
    let apiKeySaveTimer: ReturnType<typeof setTimeout> | null = null
    // Captured at schedule time so a switch-provider-mid-typing flushes against the right key.
    let pendingApiKeySave: { providerId: string; value: string } | null = null

    // Event listeners cleanup
    const unlistenFns: Array<() => void> = []

    // Load current cloud provider config into local state, then populate the model list on open
    // (cache hit → instant; cold + checkable → one debounced check). The API key is async (lives in
    // the OS secret store, not settings.json) so the field shows the saved model immediately while
    // the key fetch resolves; the model list fills in once we know the config is checkable.
    onMount(() => {
        void loadCloudProviderConfig(cloudProviderId).then(() => void populateModelsOnOpen())
    })

    // Subscribe to cloud provider changes
    const unsubCloudProvider = onSpecificSettingChange('ai.cloudProvider', (_id, newValue) => {
        // Commit any in-flight typing to the OLD provider's keychain entry before we switch;
        // otherwise the pending save would silently target the wrong provider after `cloudProviderId`
        // changes below.
        flushPendingApiKeySave()
        cloudProviderId = newValue
        void loadCloudProviderConfig(cloudProviderId)
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudProvider)

    // Subscribe to cloud provider configs changes (push to backend)
    const unsubCloudConfigs = onSpecificSettingChange('ai.cloudProviderConfigs', () => {
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudConfigs)

    // When the Ask Cmdr slot has its own model, this section's model doesn't reach Ask
    // Cmdr — say so under the picker instead of letting the change silently not apply.
    let askCmdrModelOverride = $state(getSetting('askCmdr.interactiveModel').trim())
    const unsubAskCmdrModel = onSpecificSettingChange('askCmdr.interactiveModel', (_id, newValue) => {
        askCmdrModelOverride = newValue.trim()
    })
    unlistenFns.push(unsubAskCmdrModel)

    onDestroy(() => {
        // Flush any in-flight typing before tearing down: closing Settings (or navigating to a
        // different section) shouldn't drop a key the user already typed.
        flushPendingApiKeySave()
        for (const fn of unlistenFns) {
            fn()
        }
        if (connectionCheckTimer) {
            clearTimeout(connectionCheckTimer)
        }
    })

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

        // Capture the config we're checking so we can cache the result under the right fingerprint
        // even if the user keeps typing while the request is in flight.
        const baseUrlAtStart = resolvedBaseUrl
        const apiKeyAtStart = currentApiKey
        const providerIdAtStart = cloudProviderId

        connectionStatus = 'checking'
        connectionError = null
        // Keep the prior list during a refetch: the field text is `inputValue`-driven, but a
        // flashing-empty suggestion list mid-check is a regression we forbid (finding #4).

        try {
            const result = await checkAiConnection(baseUrlAtStart, apiKeyAtStart)

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
                void cacheModels(providerIdAtStart, baseUrlAtStart, apiKeyAtStart, result.models)
            } else {
                connectionStatus = 'connected-no-models'
            }
        } catch (e) {
            connectionStatus = 'error'
            connectionError = e instanceof Error ? e.message : tString('ai.cloud.unknownError')
        }
    }

    /**
     * On open, serve the model list from the session cache instantly, or kick off a check that
     * fills it (dev and prod both auto-load; only automated E2E is suppressed, since it has no real
     * provider). A warm cache hit still works everywhere, including E2E. Also skips when a check is
     * already scheduled (for example from a just-handled provider switch) so we don't double-fire.
     */
    async function populateModelsOnOpen(): Promise<void> {
        if (!hasCheckableConfig) return
        const fingerprint = await computeModelCacheKey(cloudProviderId, resolvedBaseUrl, currentApiKey)
        const cached = getCachedModels(fingerprint)
        if (cached) {
            availableModels = cached
            connectionStatus = 'connected'
            return
        }
        // Auto-loading the list is the only request that fires without a user action; suppress it
        // only in automated E2E (no real provider there, so it'd just add network flakiness). Dev and
        // prod both auto-load. Cache hits above still work everywhere, including E2E.
        if (getAppMode() === 'e2e') return
        if (connectionCheckTimer || connectionStatus === 'checking') return
        scheduleConnectionCheck()
    }

    async function cacheModels(providerId: string, baseUrl: string, apiKey: string, models: string[]): Promise<void> {
        const fingerprint = await computeModelCacheKey(providerId, baseUrl, apiKey)
        setCachedModels(fingerprint, models)
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

    async function loadCloudProviderConfig(providerId: string): Promise<void> {
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const providerConfig = configs[providerId]
        const preset = getCloudProvider(providerId)

        currentModel = providerConfig?.model ?? preset?.defaultModel ?? ''
        currentBaseUrl =
            providerId === 'custom' || providerId === 'azure-openai'
                ? (providerConfig?.baseUrl ?? preset?.baseUrl ?? '')
                : (preset?.baseUrl ?? '')

        // Reset eagerly so a stale key from the previous provider doesn't flash while the secret
        // store read is in flight.
        currentApiKey = ''
        clearSecretError()
        await loadApiKeyForProvider(providerId)
    }

    async function loadApiKeyForProvider(providerId: string): Promise<void> {
        try {
            const fetched = await getAiApiKey(providerId)
            // Bail out if the user switched providers again before the fetch resolved.
            if (providerId !== cloudProviderId) return
            currentApiKey = fetched
        } catch (e) {
            if (providerId !== cloudProviderId) return
            // Empty key is the right user-visible state when the read fails so the user can re-enter.
            // We surface the failure inline + via toast so the cause is actionable.
            setSecretError(describeSecretError(e, 'read'))
        }
    }

    function saveCloudProviderField(field: 'model' | 'baseUrl', value: string): void {
        const configsJson = getSetting('ai.cloudProviderConfigs')
        const configs = getProviderConfigs(configsJson)
        const existing = configs[cloudProviderId] ?? { model: '' }

        if (field === 'model') existing.model = value
        else existing.baseUrl = value

        const newJson = setProviderConfig(configsJson, cloudProviderId, existing)
        setSetting('ai.cloudProviderConfigs', newJson)

        // Trigger debounced connection check on base URL change (model changes don't affect connectivity).
        if (field === 'baseUrl') {
            scheduleConnectionCheck()
        }
    }

    function handleApiKeyChange(value: string): void {
        // Reflect the typed value locally so the input stays in sync regardless of save outcome.
        currentApiKey = value
        clearSecretError()
        // Capture the provider at schedule time. If the user switches providers before the timer
        // fires, the trailing keystroke from the previous provider still targets the right entry.
        pendingApiKeySave = { providerId: cloudProviderId, value }
        if (apiKeySaveTimer) clearTimeout(apiKeySaveTimer)
        apiKeySaveTimer = setTimeout(() => {
            const pending = pendingApiKeySave
            apiKeySaveTimer = null
            pendingApiKeySave = null
            if (pending) void persistApiKey(pending.providerId, pending.value)
        }, API_KEY_SAVE_DEBOUNCE_MS)
    }

    /** Immediately commit any pending API key save. Idempotent, safe to call when nothing's queued. */
    function flushPendingApiKeySave(): void {
        if (!apiKeySaveTimer || !pendingApiKeySave) return
        clearTimeout(apiKeySaveTimer)
        const pending = pendingApiKeySave
        apiKeySaveTimer = null
        pendingApiKeySave = null
        void persistApiKey(pending.providerId, pending.value)
    }

    async function persistApiKey(providerId: string, value: string): Promise<void> {
        try {
            await saveAiApiKey(providerId, value)
        } catch (e) {
            // Failed to persist: surface it visibly and SKIP pushing config + scheduling the
            // connection check. The in-memory value would mislead the user into thinking it worked.
            setSecretError(describeSecretError(e, 'save'))
            return
        }
        // Only sync the backend if the user is still on this provider. Otherwise the new
        // provider's pushConfigToBackend (triggered by the switch) is the authoritative push.
        if (providerId !== cloudProviderId) return
        void pushConfigToBackend()
        scheduleConnectionCheck()
    }

    function setSecretError(msg: SecretErrorMessage): void {
        secretError = msg
        const body = msg.body ? `\n${msg.body}` : ''
        addToast(`${msg.title}${body}`, {
            level: msg.level,
            dismissal: 'persistent',
            id: secretErrorToastId,
        })
    }

    function clearSecretError(): void {
        if (secretError !== null) {
            secretError = null
            dismissToast(secretErrorToastId)
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

    function handleModelInputChange(model: string): void {
        currentModel = model
        saveCloudProviderField('model', model)
    }

    // Derived state
    const currentPreset = $derived(getCloudProvider(cloudProviderId))
    const providerSelectItems = $derived<SelectItem[]>(
        cloudProviderPresets.map((preset) => ({ value: preset.id, label: preset.name })),
    )
    const modelComboboxItems = $derived<ComboboxItem[]>(availableModels.map((m) => ({ value: m, label: m })))
    const modelPlaceholder = $derived(
        currentPreset?.defaultModel
            ? tString('ai.cloud.modelPlaceholderExample', { model: currentPreset.defaultModel })
            : tString('ai.cloud.modelPlaceholderGeneric'),
    )
    const showEditableBaseUrl = $derived(cloudProviderId === 'custom' || cloudProviderId === 'azure-openai')
    const resolvedBaseUrl = $derived(showEditableBaseUrl ? currentBaseUrl : (currentPreset?.baseUrl ?? ''))
    const requiresApiKey = $derived(currentPreset?.requiresApiKey ?? false)
    const hasCheckableConfig = $derived(requiresApiKey ? currentApiKey !== '' : resolvedBaseUrl !== '')
    const apiKeyPlaceholder = $derived(
        cloudProviderId === 'openai'
            ? tString('ai.cloud.apiKeyPlaceholderOpenai')
            : cloudProviderId === 'anthropic'
              ? tString('ai.cloud.apiKeyPlaceholderAnthropic')
              : tString('ai.cloud.apiKeyPlaceholderGeneric'),
    )
</script>

<SectionCard>
    {#if shouldShow('ai.cloudProvider')}
        <SettingRow
            id="ai.cloudProvider"
            label={tString('settings.ai.cloudProvider.label')}
            description={tString('settings.ai.cloudProvider.description')}
            split
            {searchQuery}
        >
            <Select
                items={providerSelectItems}
                value={cloudProviderId}
                onChange={handleCloudProviderChange}
                ariaLabel={tString('ai.cloud.serviceAria')}
            />
        </SettingRow>
        {#if currentPreset?.description}
            <p class="provider-description">{currentPreset.description}</p>
        {/if}
    {/if}

    <SettingRow
        id="ai.cloudProviderConfigs"
        label={tString('ai.cloud.endpointLabel')}
        description={tString('ai.cloud.endpointDescription')}
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
                placeholder={tString('ai.cloud.endpointPlaceholder')}
                aria-label={tString('ai.cloud.endpointAria')}
                autocomplete="off"
                spellcheck="false"
            />
        {:else}
            <input
                class="text-input text-input-readonly"
                type="text"
                value={resolvedBaseUrl}
                readonly
                aria-label={tString('ai.cloud.endpointAria')}
                tabindex="-1"
            />
        {/if}
    </SettingRow>

    {#if requiresApiKey}
        <SettingRow
            id="ai.cloudProviderConfigs"
            label={tString('ai.cloud.apiKeyLabel')}
            description={tString('ai.cloud.apiKeyDescription')}
            split
            {searchQuery}
        >
            <SettingPasswordInput
                id="ai.cloudProviderConfigs"
                placeholder={apiKeyPlaceholder}
                ariaLabel={tString('ai.cloud.apiKeyLabel')}
                value={currentApiKey}
                onchange={handleApiKeyChange}
            />
        </SettingRow>
    {/if}

    <SettingRow
        id="ai.cloudProviderConfigs"
        label={tString('ai.cloud.modelLabel')}
        description={tString('ai.cloud.modelDescription')}
        split
        {searchQuery}
    >
        <Combobox
            items={modelComboboxItems}
            inputValue={currentModel}
            onInputValueChange={handleModelInputChange}
            loading={connectionStatus === 'checking'}
            placeholder={modelPlaceholder}
            ariaLabel={tString('ai.cloud.modelLabel')}
        />
    </SettingRow>

    {#if askCmdrModelOverride}
        <p class="askcmdr-override-hint" role="note">
            {tString('ai.cloud.askCmdrOverrideHint', { model: askCmdrModelOverride })}
        </p>
    {/if}

    <!-- Connection status -->
    {#if secretError}
        <div class="secret-error" role="alert">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="secret-error-text">
                <span class="secret-error-title">{secretError.title}</span>
                {#if secretError.body}
                    <span class="secret-error-body">{secretError.body}</span>
                {/if}
            </span>
        </div>
    {/if}

    {#if connectionStatus === 'checking'}
        <div class="connection-status">
            <Spinner size="sm" />
            <span class="connection-status-text">{tString('ai.cloud.checking')}</span>
        </div>
    {:else if connectionStatus === 'connected'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-ok">&#x2713;</span>
            <span class="connection-status-text">{tString('ai.cloud.connected')}</span>
            <Button size="mini" onclick={() => void triggerConnectionCheck()}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if connectionStatus === 'connected-no-models'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-ok">&#x2713;</span>
            <span class="connection-status-text">{tString('ai.cloud.connectedNoModels')}</span>
            <Button size="mini" onclick={() => void triggerConnectionCheck()}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if connectionStatus === 'auth-error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{connectionError ?? tString('ai.cloud.authError')}</span
            >
            <Button size="mini" onclick={() => void triggerConnectionCheck()}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if connectionStatus === 'connection-error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{connectionError ?? tString('ai.cloud.connectionError')}</span
            >
            <Button size="mini" onclick={() => void triggerConnectionCheck()}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if connectionStatus === 'error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{connectionError ?? tString('ai.cloud.genericError')}</span
            >
            <Button size="mini" onclick={() => void triggerConnectionCheck()}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if connectionStatus === 'idle' && hasCheckableConfig}
        <div class="connection-status">
            <Button size="mini" onclick={() => void triggerConnectionCheck()}
                >{tString('ai.cloud.testConnection')}</Button
            >
        </div>
    {/if}
</SectionCard>

<style>
    .provider-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
        line-height: 1.4;
    }

    .askcmdr-override-hint {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
        line-height: 1.4;
        overflow-wrap: anywhere;
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

    .secret-error {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) 0;
        font-size: var(--font-size-sm);
    }

    .secret-error-text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .secret-error-title {
        color: var(--color-error);
        font-weight: 600;
    }

    .secret-error-body {
        color: var(--color-text-secondary);
    }
</style>
