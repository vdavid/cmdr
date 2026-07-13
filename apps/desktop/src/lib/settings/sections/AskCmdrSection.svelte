<!--
  The Ask Cmdr settings section: the enable/consent toggle, the "what Ask Cmdr sends"
  disclosure (the same human-reviewed copy as the opt-in screen), the provider/model
  (interactive slot), and the spend rollup. The enable state is consent, stored
  in `main.db` (not a preference), so it's driven by the consent commands, not the registry.
-->
<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getSetting, setSetting, onSpecificSettingChange, getSettingDefinition, type AiProvider } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getAppLogger } from '$lib/logging/logger'
    import { askCmdrCostSummary, type CostSummary } from '$lib/tauri-commands'
    import { consentState, refreshConsent, acceptConsent, revokeConsent } from '$lib/ask-cmdr/ask-cmdr-consent.svelte'
    import { formatUsdMicros } from '$lib/ask-cmdr/ask-cmdr-cost'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()
    const shouldShow = $derived(createShouldShow(searchQuery))
    const log = getAppLogger('askCmdr')

    // Enable state = consent (main.db). Refresh on mount so the toggle reflects the store,
    // even if the rail changed it in the main window.
    let busy = $state(false)
    $effect(() => {
        void refreshConsent()
    })
    const enabled = $derived(consentState.accepted === true)

    async function toggle(): Promise<void> {
        if (busy) return
        busy = true
        try {
            if (enabled) await revokeConsent()
            else await acceptConsent()
        } finally {
            busy = false
        }
    }

    // Which AI provider Ask Cmdr shares (Off / Cloud AI / Local LLM), reactive to the AI
    // settings section.
    const providerLabelKey: Record<AiProvider, MessageKey> = {
        off: 'settings.ai.provider.opt.off',
        cloud: 'settings.ai.provider.opt.cloud',
        local: 'settings.ai.provider.opt.local',
    }
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    $effect(() => onSpecificSettingChange('ai.provider', (_id, v) => { provider = v }))

    // The interactive-slot model override (a hand-rolled text row: the registry has no
    // generic text-input primitive). Seed from the store, keep in sync cross-window.
    const modelDef = getSettingDefinition('askCmdr.interactiveModel') ?? { label: '', description: '' }
    let model = $state(getSetting('askCmdr.interactiveModel'))
    $effect(() => onSpecificSettingChange('askCmdr.interactiveModel', (_id, v) => { model = v }))
    function onModelInput(event: Event): void {
        const value = (event.target as HTMLInputElement).value
        model = value
        setSetting('askCmdr.interactiveModel', value)
    }

    // The per-day spend rollup (loaded on mount; refreshed when the section re-enables).
    let spend = $state<CostSummary | null>(null)
    $effect(() => {
        void enabled // reload after turning on, so a first chat's cost appears
        void askCmdrCostSummary().then(
            (s) => { spend = s },
            (e: unknown) => { log.warn('reading spend failed: {error}', { error: String(e) }) },
        )
    })

    // The cost half of one day's row: honest miss-path (unknown before free; a zero-cost
    // fully-priced day is local/on-device).
    function dayCostText(day: CostSummary['days'][number]): string {
        if (!day.fullyPriced) return tString('askCmdr.cost.unknown')
        if (day.costMicros > 0) return tString('askCmdr.cost.estimate', { amount: formatUsdMicros(day.costMicros) })
        return tString('askCmdr.cost.free')
    }

    // Local ISO date (YYYY-MM-DD) for the "on since" line, style-preferred and locale-safe.
    function localIsoDate(unixSecs: number): string {
        const d = new Date(unixSecs * 1000)
        const pad = (n: number): string => String(n).padStart(2, '0')
        return `${String(d.getFullYear())}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
    }
</script>

<SettingsSection title={tString('settings.section.askCmdr')}>
    <p class="intro">{tString('settings.askCmdr.intro')}</p>

    <!-- Enable / consent -->
    <div class="enable-row">
        <div class="enable-status">
            <span class="status-label">
                {tString(enabled ? 'settings.askCmdr.status.on' : 'settings.askCmdr.status.off')}
            </span>
            {#if enabled && consentState.acceptedAt}
                <span class="status-since">
                    {tString('settings.askCmdr.status.onSince', { date: localIsoDate(consentState.acceptedAt) })}
                </span>
            {/if}
        </div>
        <Button variant={enabled ? 'secondary' : 'primary'} disabled={busy} onclick={() => void toggle()}>
            {tString(enabled ? 'settings.askCmdr.turnOff' : 'settings.askCmdr.turnOn')}
        </Button>
    </div>

    <!-- What Ask Cmdr sends (the same copy as the opt-in screen) -->
    <details class="disclosure">
        <summary>{tString('settings.askCmdr.disclosure.title')}</summary>
        <div class="disclosure-body">
            <p>{tString('askCmdr.consent.intro')}</p>
            <ul>
                <li>{tString('askCmdr.consent.item.messages')}</li>
                <li>{tString('askCmdr.consent.item.names')}</li>
                <li>{tString('askCmdr.consent.item.sizes')}</li>
                <li>{tString('askCmdr.consent.item.envelope')}</li>
                <li>{tString('askCmdr.consent.item.attachments')}</li>
            </ul>
            <p>{tString('askCmdr.consent.noContents')}</p>
            <p>{tString('askCmdr.consent.local')}</p>
            <p class="fine">{tString('askCmdr.consent.logsNote')}</p>
        </div>
    </details>

    <!-- Provider + model (the interactive slot over the shared ai/ config) -->
    <h3 class="group-title">{tString('settings.askCmdr.provider.title')}</h3>
    {#if provider === 'off'}
        <p class="provider-hint">{tString('settings.askCmdr.provider.off')}</p>
    {:else}
        <p class="provider-hint">
            {tString('settings.askCmdr.provider.shared', { provider: tString(providerLabelKey[provider]) })}
        </p>
    {/if}
    {#if shouldShow('askCmdr.interactiveModel')}
        <SettingRow
            id="askCmdr.interactiveModel"
            label={modelDef.label}
            description={modelDef.description}
            split
            {searchQuery}
        >
            <input
                type="text"
                class="model-input"
                value={model}
                placeholder={tString('settings.askCmdr.interactiveModel.placeholder')}
                oninput={onModelInput}
            />
        </SettingRow>
    {/if}

    <!-- Spend -->
    <h3 class="group-title">{tString('settings.askCmdr.spend.title')}</h3>
    {#if spend && spend.days.length > 0}
        <ul class="spend-list">
            {#each spend.days as day (day.day)}
                <li class="spend-row">
                    <span class="spend-day">{day.day}</span>
                    <span class="spend-tokens">
                        {tString('askCmdr.cost.tokens', {
                            count: day.promptTokens + day.completionTokens,
                            countText: formatInteger(day.promptTokens + day.completionTokens),
                        })}
                    </span>
                    <span class="spend-cost">{dayCostText(day)}</span>
                </li>
            {/each}
        </ul>
        <p class="fine">{tString('settings.askCmdr.spend.disclaimer')}</p>
    {:else}
        <p class="provider-hint">{tString('settings.askCmdr.spend.empty')}</p>
    {/if}
</SettingsSection>

<style>
    .intro {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .enable-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) 0;
    }

    .enable-status {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .status-label {
        font-size: var(--font-size-md);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .status-since {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .disclosure {
        margin: var(--spacing-sm) 0 var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .disclosure summary {
        cursor: default;
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .disclosure-body {
        margin-top: var(--spacing-sm);
        line-height: 1.5;
    }

    .disclosure-body ul {
        padding-left: var(--spacing-lg);
    }

    .disclosure-body p {
        margin: 0 0 var(--spacing-sm);
    }

    .fine {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .group-title {
        margin: var(--spacing-lg) 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .provider-hint {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .model-input {
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .spend-list {
        margin: 0 0 var(--spacing-sm);
        padding: 0;
        list-style: none;
    }

    .spend-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .spend-day {
        flex: none;
        width: 6.5rem;
        color: var(--color-text-primary);
    }

    .spend-tokens {
        flex: 1;
    }

    .spend-cost {
        flex: none;
    }
</style>
