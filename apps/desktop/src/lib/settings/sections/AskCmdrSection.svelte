<!--
  The Ask Cmdr settings section: the on/off switch (`askCmdr.enabled`, a plain setting), the
  provider/model (interactive slot), the proactive loop, the memory controls, and the spend
  rollup. Consent to send anything to a cloud service isn't here: it's the Allow cloud AI
  switch in Settings > AI > Provider (`#lib/ai/`), and this section only points there.
-->
<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import Button from '#lib/ui/Button.svelte'
    import StatusBadge from '#lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '#lib/feature-status.js'
    import { getSetting, onSpecificSettingChange, getSettingDefinition, type AiProvider } from '#lib/settings/index.js'
    import { anyVisible, createShouldShow } from '#lib/settings/settings-search.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import SettingTextInput from '../components/SettingTextInput.svelte'
    import { formatDuration, seconds } from '#lib/units/index.js'
    import {
        askCmdrCostSummary,
        askCmdrForgetMemory,
        askCmdrMemoryFolder,
        askCmdrModelWindow,
        onAiModelCapabilitiesChanged,
        requestRevealPath,
        type CostSummary,
    } from '#lib/tauri-commands/index.js'
    import { cloudAiBlocked, openCloudConsentSettings, refreshCloudConsent } from '#lib/ai/cloud-consent.svelte.js'
    import { getManagedPolicyView } from '#lib/managed-policy/managed-policy.svelte.js'
    import { costEstimateText, exactTokensText } from '#lib/ask-cmdr/ask-cmdr-cost.js'
    import ForgetMemoryDialog from './ForgetMemoryDialog.svelte'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { dependOn } from '#lib/utils/reactivity.js'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()
    const shouldShow = $derived(createShouldShow(searchQuery))
    const log = getAppLogger('askCmdr')

    const askCmdrBadge = getBadgeStatus('ask-cmdr')

    // The on/off switch and the "cloud AI is off" hint under it. Consent is read here only to
    // say why an enabled Ask Cmdr can't chat yet; the switch that grants it lives in AI settings.
    const enabledDef = getSettingDefinition('askCmdr.enabled') ?? { label: '', description: '' }
    let enabled = $state(getSetting('askCmdr.enabled'))
    $effect(() => onSpecificSettingChange('askCmdr.enabled', (v) => { enabled = v }))
    $effect(() => {
        void refreshCloudConsent()
    })

    // Which AI provider Ask Cmdr shares (Off / Cloud AI / Local LLM), reactive to the AI
    // settings section.
    const providerLabelKey: Record<AiProvider, MessageKey> = {
        off: 'settings.ai.provider.opt.off',
        cloud: 'settings.ai.provider.opt.cloud',
        local: 'settings.ai.provider.opt.local',
    }
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    $effect(() => onSpecificSettingChange('ai.provider', (v) => { provider = v }))

    // The interactive-slot model override. The section also follows it because the
    // model-window warning below derives from the current value.
    const modelDef = getSettingDefinition('askCmdr.interactiveModel') ?? { label: '', description: '' }
    let model = $state(getSetting('askCmdr.interactiveModel'))
    $effect(() => onSpecificSettingChange('askCmdr.interactiveModel', (v) => { model = v }))

    // Chat memory size: the presets live in the registry, the warning needs the model's window.
    // The window comes from the backend (one family table, not two), and the COMPARISON happens
    // here, against the value the user just picked: a stored setting reaches Rust up to half a
    // second later, so asking the backend "is my pick too big" would warn a beat late.
    const memoryDef = getSettingDefinition('askCmdr.chatMemorySize') ?? { label: '', description: '' }
    let chatMemorySize = $state(getSetting('askCmdr.chatMemorySize'))
    $effect(() => onSpecificSettingChange('askCmdr.chatMemorySize', (v) => { chatMemorySize = v }))
    let knownWindowTokens = $state<number | null>(null)
    // The provider's own report may land after mount (fetched in the background), and it beats
    // the table, so its arrival re-reads the window too.
    let capabilitiesVersion = $state(0)
    $effect(() => {
        const unlisten = onAiModelCapabilitiesChanged(() => { capabilitiesVersion += 1 })
        return () => { void unlisten.then((fn) => { fn() }) }
    })
    // The reply's ceiling comes out of the same window, and the Effort pick moves it.
    let replyTokens = $state(0)
    let reasoningEfforts = $state(getSetting('ai.reasoningEfforts'))
    $effect(() => onSpecificSettingChange('ai.reasoningEfforts', (v) => { reasoningEfforts = v }))
    $effect(() => {
        dependOn(provider, model, capabilitiesVersion, reasoningEfforts) // each one can move what this compares against
        void askCmdrModelWindow().then(
            (w) => {
                knownWindowTokens = w.knownWindowTokens
                replyTokens = w.replyTokens
            },
            (e: unknown) => { log.warn('reading the model window failed: {error}', { error: e }) },
        )
    })
    // Only an explicit size can exceed a window; "Automatic" follows it by construction. An
    // unknown window (a model Cmdr has no row for) stays quiet rather than guessing. The reply
    // counts: a provider refuses a prompt plus its `max_tokens` past the window.
    const overKnownWindow = $derived(
        chatMemorySize !== 'auto' &&
            knownWindowTokens !== null &&
            Number(chatMemorySize) + replyTokens > knownWindowTokens,
    )

    // Where one answer pauses on its estimated spend. Read fresh by the backend per send, so the
    // row only renders the registry's presets.
    const spendPauseDef = getSettingDefinition('askCmdr.spendPause') ?? { label: '', description: '' }

    // The "On its own" group: whether Ask Cmdr may start conversations, how soon it looks, and
    // whether a staged suggestion raises a notice. All three drive a SLEEPING timer in Rust, so
    // each change is pushed to the backend by `settings-applier.ts`; this section only reads.
    const proactiveDef = getSettingDefinition('askCmdr.proactive') ?? { label: '', description: '' }
    const wakeDelayDef = getSettingDefinition('askCmdr.wakeDelay') ?? { label: '', description: '' }
    const wakeToastDef = getSettingDefinition('askCmdr.wakeToast') ?? { label: '', description: '' }
    let proactive = $state(getSetting('askCmdr.proactive'))
    $effect(() => onSpecificSettingChange('askCmdr.proactive', (v) => { proactive = v }))
    let wakeDelaySeconds = $state(getSetting('askCmdr.wakeDelay'))
    $effect(() => onSpecificSettingChange('askCmdr.wakeDelay', (v) => { wakeDelaySeconds = v }))

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
    /** The wipe stopped partway (the disk refused a delete), so some notes may remain. */
    let notAllForgotten = $state(false)

    async function openMemoryFolder(): Promise<void> {
        try {
            await requestRevealPath(await askCmdrMemoryFolder())
        } catch (e: unknown) {
            log.warn('opening the memory folder failed: {error}', { error: e })
        }
    }

    async function forgetEverything(): Promise<void> {
        if (forgetting) return
        forgetting = true
        notAllForgotten = false
        try {
            const count = await askCmdrForgetMemory()
            log.info('the user cleared Ask Cmdr’s memory: {count} note(s)', { count })
            forgotten = true
        } catch (e: unknown) {
            // The backend logged the cause and how many notes went (`MemoryStore::forget_all`).
            log.warn('clearing Ask Cmdr’s memory failed: {error}', { error: e })
            forgotten = false
            notAllForgotten = true
        } finally {
            forgetting = false
            forgetOpen = false
        }
    }

    // The per-day spend rollup (loaded on mount; refreshed when the section re-enables).
    let spend = $state<CostSummary | null>(null)
    $effect(() => {
        dependOn(enabled) // reload after turning on, so a first chat's cost appears
        void askCmdrCostSummary().then(
            (s) => { spend = s },
            (e: unknown) => { log.warn('reading spend failed: {error}', { error: e }) },
        )
    })

    // The cost half of one day's row: honest miss-path (unknown before free; a zero-cost
    // fully-priced day is local/on-device).
    function dayCostText(day: CostSummary['days'][number]): string {
        if (!day.fullyPriced) return tString('askCmdr.cost.unknown')
        if (day.costMicros > 0) return costEstimateText(day.costMicros)
        return tString('askCmdr.cost.free')
    }

</script>

<SettingsSection title={tString('settings.section.askCmdr')}>
    {#snippet badge()}
        {#if askCmdrBadge}<StatusBadge status={askCmdrBadge} />{/if}
    {/snippet}
    <p class="intro">{tString('settings.askCmdr.intro')}</p>

    {#if shouldShow('askCmdr.enabled')}
        <SettingRow
            id="askCmdr.enabled"
            label={enabledDef.label}
            description={enabledDef.description}
            {searchQuery}
        >
            <SettingSwitch id="askCmdr.enabled" />
        </SettingRow>
        {#if enabled && cloudAiBlocked(provider)}
            <div class="cloud-off-hint" role="note">
                <p>{tString('settings.askCmdr.cloudOffHint')}</p>
                <Button variant="secondary" onclick={() => { openCloudConsentSettings('ask-cmdr-settings-hint'); }}>
                    {tString('settings.askCmdr.openAiSettings')}
                </Button>
            </div>
        {/if}
    {/if}

    <!-- Provider + model (the interactive slot over the shared ai/ config) -->
    <h3 class="group-title">{tString('settings.askCmdr.provider.title')}</h3>
    <!-- Under the organization's "no AI", the way back isn't in AI settings, so say who decided. -->
    {#if getManagedPolicyView().ai.mode === 'off'}
        <p class="provider-hint">{tString('ai.managed.aiOff')}</p>
    {:else if provider === 'off'}
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
            <SettingTextInput
                id="askCmdr.interactiveModel"
                placeholder={tString('settings.askCmdr.interactiveModel.placeholder')}
                ariaLabel={modelDef.label}
                onValueChange={(value: string) => {
                    model = value
                }}
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

    {#if shouldShow('askCmdr.spendPause')}
        <SettingRow
            id="askCmdr.spendPause"
            label={spendPauseDef.label}
            description={spendPauseDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="askCmdr.spendPause" />
        </SettingRow>
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

    <!-- What Cmdr remembers: the notes are about the user, so they get to read them and
         to throw them away. Neither button is a setting; both are searchable rows
         (`AskCmdrSection.rows.ts`), and the heading follows whichever one a search kept. -->
    {#if anyVisible(shouldShow, 'row:askCmdr.openMemoryFolder', 'row:askCmdr.forgetMemory')}
        <h3 class="group-title">{tString('settings.askCmdr.memory.title')}</h3>
        <p class="provider-hint">{tString('settings.askCmdr.memory.description')}</p>
        <div class="memory-actions">
            {#if shouldShow('row:askCmdr.openMemoryFolder')}
                <Button variant="secondary" onclick={() => void openMemoryFolder()}>
                    {tString('settings.askCmdr.memory.open')}
                </Button>
            {/if}
            {#if shouldShow('row:askCmdr.forgetMemory')}
                <Button variant="secondary" onclick={() => (forgetOpen = true)}>
                    {tString('askCmdr.forget.confirm')}
                </Button>
            {/if}
        </div>
        {#if forgotten}
            <p class="memory-forgotten" role="status">{tString('settings.askCmdr.memory.forgotten')}</p>
        {/if}
        {#if notAllForgotten}
            <p class="memory-not-forgotten" role="status">{tString('settings.askCmdr.memory.notAllForgotten')}</p>
        {/if}
    {/if}

    <!-- Spend -->
    <h3 class="group-title">{tString('settings.askCmdr.spend.title')}</h3>
    {#if spend && spend.days.length > 0}
        <ul class="spend-list">
            {#each spend.days as day (day.day)}
                <li class="spend-row">
                    <span class="spend-day">{day.day}</span>
                    <span class="spend-tokens">
                        {exactTokensText(day.promptTokens + day.completionTokens, day.cacheReadTokens)}
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

    .cloud-off-hint {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
        margin: var(--spacing-xs) 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .cloud-off-hint p {
        flex: 1 1 18rem;
        margin: 0;
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

    .memory-not-forgotten {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
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
