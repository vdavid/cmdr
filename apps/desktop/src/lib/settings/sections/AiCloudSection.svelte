<script lang="ts">
    import { onDestroy, untrack } from 'svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import Select, { type SelectItem } from '#lib/ui/Select.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import ProviderSetupSteps from '#lib/ai-provider-setup/ProviderSetupSteps.svelte'
    import {
        ProviderSetupController,
        type ConnectionProblem,
    } from '#lib/ai-provider-setup/provider-setup.svelte.js'
    import { followPresetHostVerdicts } from '#lib/ai-provider-setup/preset-hosts.svelte.js'
    import { managedAiRefusalMessage } from '#lib/managed-policy/ai-refusal.js'
    import { getSetting, setSetting, onSpecificSettingChange, cloudProviderPresets } from '#lib/settings/index.js'
    import { pushConfigToBackend } from '#lib/settings/ai-config.js'
    import type { SecretErrorMessage } from './ai-secret-error'
    import { addToast, dismissToast } from '#lib/ui/toast/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    /**
     * Settings › AI › Provider. The service picker, then the SAME numbered setup steps the
     * onboarding wizard shows (`#lib/ai-provider-setup/`), then the connection status.
     *
     * Everything stateful (key persist, connection check, model cache, provider-switch race
     * guards) lives in the shared controller; this file owns the settings chrome: the
     * service row, the recheck buttons, the Ask Cmdr override note, and mirroring a
     * secret-store failure into a persistent toast.
     *
     * `locked` while Allow cloud AI is off (the switch above this card): everything here is
     * `inert` and dimmed, and the controller isn't pointed at the provider until the lock
     * lifts, since that alone can start a connection check. The backend refuses the check
     * anyway (`cloudConsentMissing`); the lock keeps the UI from asking.
     */
    interface Props {
        searchQuery: string
        shouldShow: (id: string) => boolean
        locked?: boolean
    }

    const { searchQuery, shouldShow, locked = false }: Props = $props()

    let cloudProviderId = $state(getSetting('ai.cloudProvider'))

    // A secret-store failure is also re-emitted as a persistent toast, so the user can act
    // on it after closing Settings. One stable id, so repeated attempts replace in place
    // instead of stacking duplicates.
    const secretErrorToastId = 'ai-secret-store-error'

    const controller = new ProviderSetupController({
        logScope: 'ai-settings-cloud',
        onSecretErrorChange: (error: SecretErrorMessage | null) => {
            if (!error) {
                dismissToast(secretErrorToastId)
                return
            }
            const body = error.body ? `\n${error.body}` : ''
            addToast(`${error.title}${body}`, {
                level: error.level,
                dismissal: 'persistent',
                id: secretErrorToastId,
            })
        },
        onKeyChanged: () => void pushConfigToBackend(),
    })

    const unlistenFns: Array<() => void> = []

    // Point the controller at the provider once the lock is off (at mount, or when the user
    // allows cloud AI). Untracked: `setProvider` reads and writes the controller's own state,
    // so a tracked call would re-run itself (`lib/ai-provider-setup/CLAUDE.md`).
    $effect(() => {
        if (locked) return
        untrack(() => {
            if (controller.providerId !== cloudProviderId) controller.setProvider(cloudProviderId)
        })
    })

    const unsubCloudProvider = onSpecificSettingChange('ai.cloudProvider', (newValue) => {
        cloudProviderId = newValue
        // `setProvider` flushes any in-flight typing against the OLD provider's keychain
        // entry before it switches, so a trailing keystroke can't land on the wrong one.
        // While locked, the effect above picks the new id up when the lock lifts.
        if (!locked) controller.setProvider(newValue)
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudProvider)

    const unsubCloudConfigs = onSpecificSettingChange('ai.cloudProviderConfigs', () => {
        void pushConfigToBackend()
    })
    unlistenFns.push(unsubCloudConfigs)

    // When the Ask Cmdr slot has its own model, this section's model doesn't reach Ask
    // Cmdr — say so under the picker instead of letting the change silently not apply.
    let askCmdrModelOverride = $state(getSetting('askCmdr.interactiveModel').trim())
    const unsubAskCmdrModel = onSpecificSettingChange('askCmdr.interactiveModel', (newValue) => {
        askCmdrModelOverride = newValue.trim()
    })
    unlistenFns.push(unsubAskCmdrModel)

    onDestroy(() => {
        // Flush in-flight typing before teardown: closing Settings (or moving to another
        // section) shouldn't drop a key the user already typed.
        controller.destroy()
        for (const fn of unlistenFns) {
            fn()
        }
    })

    /** One line per problem kind, each a literal key so the i18n checks can see it. */
    function problemMessage(problem: ConnectionProblem): string {
        switch (problem.kind) {
            case 'keyRejected':
                return tString('ai.cloud.authError')
            case 'unreachable':
                return tString('ai.cloud.connectionError')
            case 'timedOut':
                return tString('ai.cloud.timedOut')
            case 'invalidEndpoint':
                return tString('ai.cloud.invalidEndpoint')
            case 'insecureEndpoint':
                return tString('ai.cloud.insecureEndpoint')
            case 'providerRefused':
                return tString('ai.cloud.providerRefused', { status: String(problem.status) })
            case 'checkUnavailable':
                return tString('ai.cloud.genericError')
        }
    }

    // A service the organization refuses stays listed, disabled, with the reason beside it.
    const presetHosts = followPresetHostVerdicts()
    const providerSelectItems = $derived<SelectItem[]>(
        cloudProviderPresets.map((preset) =>
            presetHosts.isRefused(preset.id)
                ? {
                      value: preset.id,
                      label: preset.name,
                      description: tString('ai.managed.serviceNotAllowed'),
                      disabled: true,
                  }
                : { value: preset.id, label: preset.name },
        ),
    )
</script>

<SectionCard>
    {#if locked}
        <p class="locked-hint">{tString('settings.ai.cloudConsent.lockedHint')}</p>
    {/if}
    <div class="cloud-setup" inert={locked}>
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
                onChange={(newProviderId: string) => { setSetting('ai.cloudProvider', newProviderId); }}
                ariaLabel={tString('ai.cloud.serviceAria')}
            />
        </SettingRow>
        {#if controller.preset?.description}
            <p class="provider-description">{controller.preset.description}</p>
        {/if}
    {/if}

    <!-- The endpoint, key, and model controls all live inside the shared steps, and all
         three are the one `ai.cloudProviderConfigs` setting, so this single gate is exactly
         the search visibility the three separate rows used to have between them. -->
    {#if shouldShow('ai.cloudProviderConfigs')}
        <div class="setup-steps-block">
            <ProviderSetupSteps {controller} idPrefix="settings-cloud" />
        </div>
    {/if}

    {#if askCmdrModelOverride}
        <p class="askcmdr-override-hint" role="note">
            {tString('ai.cloud.askCmdrOverrideHint', { model: askCmdrModelOverride })}
        </p>
    {/if}

    <!-- Connection status -->
    {#if controller.secretError}
        <div class="secret-error" role="alert">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="secret-error-text">
                <span class="secret-error-title">{controller.secretError.title}</span>
                {#if controller.secretError.body}
                    <span class="secret-error-body">{controller.secretError.body}</span>
                {/if}
            </span>
        </div>
    {/if}

    {#if controller.status === 'managed' && controller.managedRefusal}
        <p class="connection-status managed-status" role="status">
            {managedAiRefusalMessage(controller.managedRefusal)}
        </p>
    {:else if controller.status === 'checking'}
        <div class="connection-status">
            <Spinner size="sm" />
            <span class="connection-status-text">{tString('ai.cloud.checking')}</span>
        </div>
    {:else if controller.status === 'connected'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-ok">&#x2713;</span>
            <span class="connection-status-text">{tString('ai.cloud.connected')}</span>
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.status === 'connected-no-models'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-ok">&#x2713;</span>
            <span class="connection-status-text">{tString('ai.cloud.connectedNoModels')}</span>
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.problem}
        <div class="connection-status connection-problem">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="problem-text">
                <span class="connection-status-error-text">{problemMessage(controller.problem)}</span>
                {#if controller.problem.kind === 'providerRefused' && controller.problem.detail}
                    <!-- The provider's own sentence, untranslated: worth seeing, but secondary. -->
                    <span class="problem-detail">{controller.problem.detail}</span>
                {/if}
            </span>
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.status === 'idle' && controller.hasCheckableConfig}
        <div class="connection-status">
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.testConnection')}</Button>
        </div>
    {/if}
    </div>
</SectionCard>

<style>
    .locked-hint {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Dimmed while `inert` (no focus, no clicks, hidden from assistive tech), which it is
       until the user allows cloud AI. Keyed off the attribute itself, so the dimming can't
       drift from the state. */
    .cloud-setup[inert] {
        opacity: 0.5;
    }

    .provider-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
    }

    /* The steps carry their own vertical rhythm; this only keeps them off the rows
       above and below them. */
    .setup-steps-block {
        margin: var(--spacing-sm) 0 var(--spacing-md);
    }

    .askcmdr-override-hint {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: calc(-1 * var(--spacing-sm)) 0 var(--spacing-md);
        overflow-wrap: anywhere;
    }

    /* Connection status */
    .connection-status {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        font-size: var(--font-size-sm);
    }

    .managed-status {
        margin: 0;
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

    /* A two-line problem (lead line plus the provider's own detail) keeps the icon and the
       Recheck button on its first line. */
    .connection-problem {
        align-items: flex-start;
    }

    .problem-text {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .problem-detail {
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        overflow-wrap: anywhere;
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
