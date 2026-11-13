<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        unofferedSettingValue,
        type AiProvider,
    } from '#lib/settings/index.js'
    import { getAiRuntimeStatus, stopAiServer, type AiRuntimeStatus } from '#lib/tauri-commands/index.js'
    import { createShouldShow } from '#lib/settings/settings-search.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { pushConfigToBackend } from '#lib/settings/ai-config.js'
    import AiCloudSection from './AiCloudSection.svelte'
    import AiCloudConsentToggle from '#lib/ai/AiCloudConsentToggle.svelte'
    import { cloudAiBlocked } from '#lib/ai/cloud-consent.svelte.js'
    import AiLocalSection from './AiLocalSection.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getSettingLock } from '#lib/managed-policy/managed-policy.svelte.js'
    import { lockAllowsWrite } from '#lib/managed-policy/overlay.js'
    import { useSettingLock } from '../components/setting-lock.svelte'
    import { hasCapability } from '#lib/platform/index.js'

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
    // The provider the settings hold that this OS can't run (`local` on a build with no local AI,
    // from a settings file brought over from a Mac). The store reads it as `off`, so `provider`
    // above is already the real state; this is only why, in one line under the row.
    let unofferedProvider = $state(unofferedSettingValue('ai.provider'))

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
        const unsubProvider = onSpecificSettingChange('ai.provider', (newValue) => {
            const oldProvider = provider
            provider = newValue
            unofferedProvider = unofferedSettingValue('ai.provider')
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

    // Derived state. Local AI is an option only where this build ships a model runtime
    // (`localAi`); there, the backend still says whether this machine can run it (an Intel Mac
    // keeps the option, greyed out with the reason).
    const offersLocalAi = hasCapability('localAi')
    const localAiSupported = $derived(offersLocalAi && (status?.localAiSupported ?? true))

    const providerTooltips: Record<string, string> = {
        off: tString('settings.ai.tooltipOff'),
        cloud: tString('settings.ai.tooltipCloud'),
        local: tString('settings.ai.tooltipLocal'),
        'local-disabled': tString('settings.ai.tooltipLocalDisabled'),
    }

    // The organization's lock on the provider: a pinned one (AI off) disables every option and the
    // row shows the managed note; a narrowed one (on-device only) rules out Cloud alone, and says
    // so in a visible line under the row, since a tooltip on a disabled option isn't reliably read.
    const providerLock = useSettingLock('ai.provider')
    const lock = $derived(getSettingLock('ai.provider'))
    const cloudRuledOut = $derived(lock?.kind === 'disallowedValues' && !lockAllowsWrite(lock, 'cloud'))
    const managedNoteId = 'settings-ai-provider-managed'
    const managedNote = $derived(
        cloudRuledOut
            ? tString(localAiSupported ? 'ai.managed.cloudAiOff' : 'ai.managed.localOnlyUnsupported')
            : undefined,
    )

    // The organization's line says more when both apply, so it wins.
    const unofferedNoteId = 'settings-ai-provider-unoffered'
    const unofferedNote = $derived(
        unofferedProvider === 'local' && !managedNote ? tString('settings.ai.provider.localUnavailable') : undefined,
    )
    const providerNoteId = $derived(managedNote ? managedNoteId : unofferedNote ? unofferedNoteId : undefined)

    function optionDisabled(value: AiProvider): boolean {
        if (!lockAllowsWrite(lock, value)) return true
        return value === 'local' && !localAiSupported
    }

    function getProviderTooltip(value: string): string {
        if (value === 'cloud' && cloudRuledOut) return tString('ai.managed.cloudAiOff')
        if (value === 'local' && !localAiSupported) return providerTooltips['local-disabled']
        return providerTooltips[value] ?? ''
    }

    function handleProviderSelect(value: AiProvider): void {
        if (optionDisabled(value)) return
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
                    <div
                        class="provider-toggle"
                        role="radiogroup"
                        aria-label={tString('settings.ai.providerAria')}
                        aria-describedby={providerLock.describedBy(providerNoteId)}
                    >
                        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke provider segmented row: options carry per-option tooltips and a disabled "local" state a plain RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                        <button
                            class="provider-option"
                            class:selected={provider === 'off'}
                            disabled={optionDisabled('off')}
                            onclick={() => {
                                handleProviderSelect('off')
                            }}
                            use:tooltip={getProviderTooltip('off')}
                            role="radio"
                            aria-checked={provider === 'off'}
                        >
                            {tString('settings.ai.provider.opt.off')}
                        </button>
                        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke provider segmented row: options carry per-option tooltips and a disabled "local" state a plain RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                        <button
                            class="provider-option"
                            class:selected={provider === 'cloud'}
                            disabled={optionDisabled('cloud')}
                            onclick={() => {
                                handleProviderSelect('cloud')
                            }}
                            use:tooltip={getProviderTooltip('cloud')}
                            role="radio"
                            aria-checked={provider === 'cloud'}
                        >
                            {tString('settings.ai.provider.opt.cloud')}
                        </button>
                        {#if offersLocalAi}
                        <!-- eslint-disable-next-line cmdr/prefer-ui-primitive -- Bespoke provider segmented row: options carry per-option tooltips and a disabled "local" state a plain RadioGroup option list can't express; the enclosing role="radiogroup" carries the a11y contract. -->
                        <button
                            class="provider-option"
                            class:selected={provider === 'local'}
                            disabled={optionDisabled('local')}
                            onclick={() => {
                                handleProviderSelect('local')
                            }}
                            use:tooltip={getProviderTooltip('local')}
                            role="radio"
                            aria-checked={provider === 'local'}
                        >
                            {tString('settings.ai.provider.opt.local')}
                        </button>
                        {/if}
                    </div>
                </SettingRow>
                {#if managedNote}
                    <p class="provider-note" id={managedNoteId}>{managedNote}</p>
                {:else if unofferedNote}
                    <p class="provider-note" id={unofferedNoteId}>{unofferedNote}</p>
                {/if}
            </SectionCard>
        {/if}

        <!-- Cloud: the Allow cloud AI switch first, then the service setup it locks. Nothing
             here reaches the service until the switch is on (the backend enforces it too). -->
        {#if provider === 'cloud'}
            {#if shouldShow('row:ai.cloudConsent')}
                <SectionCard>
                    <!-- The group holds the switch AND its disclosure, so it carries the name. -->
                    <div role="group" aria-label={tString('ai.cloudConsent.label')}>
                        <AiCloudConsentToggle anchor />
                    </div>
                </SectionCard>
            {/if}
            <AiCloudSection {searchQuery} {shouldShow} locked={cloudAiBlocked(provider)} />
        {/if}

        <!-- Local LLM section -->
        {#if offersLocalAi && provider === 'local'}
            <AiLocalSection {searchQuery} {shouldShow} {status} />
        {/if}

        <!-- What the AI does on the "Tell me about this" card. Only with a provider on: with AI off
             the card writes no take whatever this says. -->
        {#if provider !== 'off' && shouldShow('aboutThis.aiVerdict')}
            <SectionCard>
                <SettingRow
                    id="aboutThis.aiVerdict"
                    label={tString('settings.aboutThis.aiVerdict.label')}
                    description={tString('settings.aboutThis.aiVerdict.description')}
                    {searchQuery}
                >
                    <SettingSwitch id="aboutThis.aiVerdict" />
                </SettingRow>
            </SectionCard>
        {/if}
    {/if}
</SettingsSection>

<style>
    .provider-note {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

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
