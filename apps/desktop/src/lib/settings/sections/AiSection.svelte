<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getSetting, setSetting, onSpecificSettingChange, type AiProvider } from '$lib/settings'
    import { getAiRuntimeStatus, stopAiServer, type AiRuntimeStatus } from '$lib/tauri-commands'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import AiCloudSection from './AiCloudSection.svelte'
    import AiLocalSection from './AiLocalSection.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tString } from '$lib/intl/messages.svelte'

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
        off: tString('settings.ai.tooltipOff'),
        cloud: tString('settings.ai.tooltipCloud'),
        local: tString('settings.ai.tooltipLocal'),
        'local-disabled': tString('settings.ai.tooltipLocalDisabled'),
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

<SettingsSection title={tString('settings.section.aiProvider')}>
    {#if isLoading}
        <p class="loading-text">{tString('settings.ai.loading')}</p>
    {:else}
        <!-- Provider toggle with per-option tooltips -->
        {#if shouldShow('ai.provider')}
            <SectionCard>
                <SettingRow
                    id="ai.provider"
                    label={tString('settings.ai.provider.label')}
                    description={tString('settings.ai.provider.description')}
                    {searchQuery}
                >
                    <div class="provider-toggle" role="radiogroup" aria-label={tString('settings.ai.providerAria')}>
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
                            {tString('settings.ai.provider.opt.off')}
                        </button>
                        <button
                            class="provider-option"
                            class:selected={provider === 'cloud'}
                            onclick={() => {
                                handleProviderSelect('cloud')
                            }}
                            use:tooltip={getProviderTooltip('cloud')}
                            role="radio"
                            aria-checked={provider === 'cloud'}
                        >
                            {tString('settings.ai.provider.opt.cloud')}
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
                            {tString('settings.ai.provider.opt.local')}
                        </button>
                    </div>
                </SettingRow>
            </SectionCard>
        {/if}

        <!-- Cloud / API section -->
        {#if provider === 'cloud'}
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
