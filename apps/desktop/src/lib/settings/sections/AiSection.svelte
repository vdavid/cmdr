<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        type AiProvider,
    } from '$lib/settings'
    import {
        getAiRuntimeStatus,
        configureAi,
        stopAiServer,
        type AiRuntimeStatus,
    } from '$lib/tauri-commands'
    import { resolveCloudConfig } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import AiCloudSection from './AiCloudSection.svelte'
    import AiLocalSection from './AiLocalSection.svelte'

    interface Props {
        searchQuery?: string
    }

    const { searchQuery = '' }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))
    const logger = getAppLogger('ai-settings')

    // Dynamic state from backend
    let status = $state<AiRuntimeStatus | null>(null)
    let isLoading = $state(true)

    // Track current provider for conditional rendering
    let provider = $state<AiProvider>(getSetting('ai.provider'))

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
    })

    onDestroy(() => {
        for (const fn of unlistenFns) {
            fn()
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

    async function refreshStatus(): Promise<void> {
        try {
            status = await getAiRuntimeStatus()
        } catch (e) {
            logger.error("Couldn't refresh AI status: {error}", { error: e })
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

    // Derived state
    const localAiSupported = $derived(status?.localAiSupported ?? true)

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
            <AiCloudSection {searchQuery} {shouldShow} />
        {/if}

        <!-- Local LLM section -->
        {#if provider === 'local'}
            <AiLocalSection {searchQuery} {shouldShow} {status} />
        {/if}
    {/if}
</SettingsSection>

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
</style>
