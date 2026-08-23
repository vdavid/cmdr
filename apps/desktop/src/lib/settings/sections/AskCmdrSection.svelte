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
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import { getSetting, setSetting, onSpecificSettingChange, getSettingDefinition, type AiProvider } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getAppLogger } from '$lib/logging/logger'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import { formatDuration, seconds } from '$lib/units'
    import {
        askCmdrCostSummary,
        askCmdrForgetMemory,
        askCmdrMemoryFolder,
        askCmdrModelWindow,
        requestRevealPath,
        type CostSummary,
    } from '$lib/tauri-commands'
    import { consentState, refreshConsent, acceptConsent, revokeConsent } from '$lib/ask-cmdr/ask-cmdr-consent.svelte'
    import { formatUsdMicros } from '$lib/ask-cmdr/ask-cmdr-cost'
    import ForgetMemoryDialog from './ForgetMemoryDialog.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()
    const shouldShow = $derived(createShouldShow(searchQuery))
    const log = getAppLogger('askCmdr')

    const askCmdrBadge = getBadgeStatus('ask-cmdr')

    // Enable state = consent (main.db). Refresh on mount so the toggle reflects the store,
    // even if the rail changed it in the main window.
    let busy = $state(false)
    $effect(() => {
        void refreshConsent()
    })
    const enabled = $derived(consentState.accepted === true)
    // Someone who opted in once, to wording that has since changed materially, so the bump
    // revoked them. Without this they read as "off", identical to someone who never wanted AI,
    // while a whole thread history sits behind the rail's consent screen.
    const needsReconsent = $derived(consentState.needsReconsent)

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

    // Chat memory size: the presets live in the registry, the warning needs the model's window.
    // The window comes from the backend (one family table, not two), and the COMPARISON happens
    // here, against the value the user just picked: a stored setting reaches Rust up to half a
    // second later, so asking the backend "is my pick too big" would warn a beat late.
    const memoryDef = getSettingDefinition('askCmdr.chatMemorySize') ?? { label: '', description: '' }
    let chatMemorySize = $state(getSetting('askCmdr.chatMemorySize'))
    $effect(() => onSpecificSettingChange('askCmdr.chatMemorySize', (_id, v) => { chatMemorySize = v }))
    let knownWindowTokens = $state<number | null>(null)
    $effect(() => {
        void provider // a provider or model change moves the window this compares against
        void model
        void askCmdrModelWindow().then(
            (w) => { knownWindowTokens = w.knownWindowTokens },
            (e: unknown) => { log.warn('reading the model window failed: {error}', { error: String(e) }) },
        )
    })
    // Only an explicit size can exceed a window; "Automatic" follows it by construction. An
    // unknown window (a model Cmdr has no row for) stays quiet rather than guessing.
    const overKnownWindow = $derived(
        chatMemorySize !== 'auto' && knownWindowTokens !== null && Number(chatMemorySize) > knownWindowTokens,
    )

    // The "On its own" group: whether Ask Cmdr may start conversations, how soon it looks, and
    // whether a staged suggestion raises a notice. All three drive a SLEEPING timer in Rust, so
    // each change is pushed to the backend by `settings-applier.ts`; this section only reads.
    const proactiveDef = getSettingDefinition('askCmdr.proactive') ?? { label: '', description: '' }
    const wakeDelayDef = getSettingDefinition('askCmdr.wakeDelay') ?? { label: '', description: '' }
    const wakeToastDef = getSettingDefinition('askCmdr.wakeToast') ?? { label: '', description: '' }
    let proactive = $state(getSetting('askCmdr.proactive'))
    $effect(() => onSpecificSettingChange('askCmdr.proactive', (_id, v) => { proactive = v }))
    let wakeDelaySeconds = $state(getSetting('askCmdr.wakeDelay'))
    $effect(() => onSpecificSettingChange('askCmdr.wakeDelay', (_id, v) => { wakeDelaySeconds = v }))

    // How long a quieter folder waits: a minute of patience for every second of attentiveness,
    // held to six hours. ⚠️ Mirrors `agent::wake::interest`'s `WARM_MULTIPLE` / `MAX_WARM_DELAY`,
    // which is what actually schedules the wake; change the two together or the row lies.
    const WARM_MULTIPLE = 60
    const MAX_WARM_DELAY_SECONDS = 6 * 60 * 60

    /** The cadence readout, in the same compact form every ETA in the app uses. */
    function wakeDuration(secs: number): string {
        return formatDuration(seconds(secs))
    }

    // The description names BOTH waits, so it can't come from `descriptionKey` (a static
    // string). The registry keeps that static text for the search index; the rendered row gets
    // this, with the two durations already formatted.
    const wakeDelaySummary = $derived(
        tString('settings.askCmdr.wakeDelay.summary', {
            hot: wakeDuration(wakeDelaySeconds),
            warm: wakeDuration(Math.min(wakeDelaySeconds * WARM_MULTIPLE, MAX_WARM_DELAY_SECONDS)),
        }),
    )

    // The memory controls. "Open memory folder" needs the path from Rust (it moves with
    // `CMDR_DATA_DIR`), and this window has no panes, so the main window is asked to show it.
    let forgetOpen = $state(false)
    let forgetting = $state(false)
    let forgotten = $state(false)

    async function openMemoryFolder(): Promise<void> {
        try {
            await requestRevealPath(await askCmdrMemoryFolder())
        } catch (e: unknown) {
            log.warn('opening the memory folder failed: {error}', { error: String(e) })
        }
    }

    async function forgetEverything(): Promise<void> {
        if (forgetting) return
        forgetting = true
        try {
            const count = await askCmdrForgetMemory()
            log.info('the user cleared Ask Cmdr’s memory: {count} note(s)', { count })
            forgotten = true
        } catch (e: unknown) {
            log.warn('clearing Ask Cmdr’s memory failed: {error}', { error: String(e) })
        } finally {
            forgetting = false
            forgetOpen = false
        }
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
    {#snippet badge()}
        {#if askCmdrBadge}<StatusBadge status={askCmdrBadge} />{/if}
    {/snippet}
    <p class="intro">{tString('settings.askCmdr.intro')}</p>

    <!-- Enable / consent. Three states, not two: "paused" is somebody who said yes to
         wording that has since changed, and a bare "off" at them loses that entirely. -->
    <div class="enable-row">
        <div class="enable-status">
            <span class="status-label">
                {#if enabled}
                    {tString('settings.askCmdr.status.on')}
                {:else if needsReconsent}
                    {tString('settings.askCmdr.status.needsReview')}
                {:else}
                    {tString('settings.askCmdr.status.off')}
                {/if}
            </span>
            {#if enabled && consentState.acceptedAt}
                <span class="status-since">
                    {tString('settings.askCmdr.status.onSince', { date: localIsoDate(consentState.acceptedAt) })}
                </span>
            {:else if needsReconsent}
                <span class="status-changed">{tString('settings.askCmdr.status.changed')}</span>
            {/if}
        </div>
        <Button variant={enabled ? 'secondary' : 'primary'} disabled={busy} onclick={() => void toggle()}>
            {#if enabled}
                {tString('settings.askCmdr.turnOff')}
            {:else if needsReconsent}
                {tString('settings.askCmdr.turnBackOn')}
            {:else}
                {tString('settings.askCmdr.turnOn')}
            {/if}
        </Button>
    </div>

    <!-- What Ask Cmdr sends (the same copy as the opt-in screen). Open by default for
         somebody being asked again: the button above says "read what's new below". -->
    <details class="disclosure" open={needsReconsent}>
        <summary>{tString('settings.askCmdr.disclosure.title')}</summary>
        <div class="disclosure-body">
            {#if needsReconsent}
                <p class="changed-lede">{tString('askCmdr.consent.whatsNew.body')}</p>
            {/if}
            <p>{tString('askCmdr.consent.intro')}</p>
            <ul>
                <li>{tString('askCmdr.consent.item.messages')}</li>
                <li>{tString('askCmdr.consent.item.names')}</li>
                <li>{tString('askCmdr.consent.item.sizes')}</li>
                <li>{tString('askCmdr.consent.item.envelope')}</li>
                <li>{tString('askCmdr.consent.item.attachments')}</li>
                <li>{tString('askCmdr.consent.item.memory')}</li>
            </ul>
            <p>{tString('askCmdr.consent.noContents')}</p>
            <p>{tString('askCmdr.consent.memory')}</p>
            <p>{tString('askCmdr.consent.proactive')}</p>
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
            <TextInput
                value={model}
                placeholder={tString('settings.askCmdr.interactiveModel.placeholder')}
                ariaLabel={modelDef.label}
                oninput={onModelInput}
            />
        </SettingRow>
    {/if}

    {#if shouldShow('askCmdr.chatMemorySize')}
        <SettingRow
            id="askCmdr.chatMemorySize"
            label={memoryDef.label}
            description={memoryDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="askCmdr.chatMemorySize" />
        </SettingRow>
        {#if overKnownWindow}
            <p class="memory-warning" role="status">{tString('settings.askCmdr.chatMemorySize.overWindow')}</p>
        {/if}
    {/if}

    <!-- On its own: the proactive loop's three knobs -->
    <h3 class="group-title">{tString('settings.askCmdr.proactive.title')}</h3>
    {#if shouldShow('askCmdr.proactive')}
        <SettingRow
            id="askCmdr.proactive"
            label={proactiveDef.label}
            description={proactiveDef.description}
            split
            {searchQuery}
        >
            <SettingSwitch id="askCmdr.proactive" />
        </SettingRow>
    {/if}

    {#if shouldShow('askCmdr.wakeDelay')}
        <SettingRow
            id="askCmdr.wakeDelay"
            label={wakeDelayDef.label}
            description={wakeDelaySummary}
            split
            {searchQuery}
        >
            <SettingSlider id="askCmdr.wakeDelay" disabled={!proactive} formatValue={wakeDuration} />
        </SettingRow>
    {/if}

    {#if shouldShow('askCmdr.wakeToast')}
        <SettingRow
            id="askCmdr.wakeToast"
            label={wakeToastDef.label}
            description={wakeToastDef.description}
            split
            {searchQuery}
        >
            <SettingSwitch id="askCmdr.wakeToast" disabled={!proactive} />
        </SettingRow>
    {/if}

    <!-- What Ask Cmdr remembers: the notes are about the user, so they get to read them and
         to throw them away. -->
    <h3 class="group-title">{tString('settings.askCmdr.memory.title')}</h3>
    <p class="provider-hint">{tString('settings.askCmdr.memory.description')}</p>
    <div class="memory-actions">
        <Button variant="secondary" onclick={() => void openMemoryFolder()}>
            {tString('settings.askCmdr.memory.open')}
        </Button>
        <Button variant="secondary" onclick={() => (forgetOpen = true)}>
            {tString('askCmdr.forget.confirm')}
        </Button>
    </div>
    {#if forgotten}
        <p class="memory-forgotten" role="status">{tString('settings.askCmdr.memory.forgotten')}</p>
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

{#if forgetOpen}
    <ForgetMemoryDialog
        isForgetting={forgetting}
        onConfirm={() => void forgetEverything()}
        onCancel={() => (forgetOpen = false)}
    />
{/if}

<style>
    .intro {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
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

    .status-changed {
        max-width: 36rem;
        font-size: var(--font-size-xs);
        line-height: var(--font-line-height-prose);
        color: var(--color-warning-text);
    }

    .changed-lede {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .memory-actions {
        display: flex;
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) 0;
    }

    .memory-forgotten {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
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
    }

    .memory-warning {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
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
