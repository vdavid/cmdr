<script lang="ts">
    import { onDestroy, onMount } from 'svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ProviderSetupSteps from '$lib/ai-provider-setup/ProviderSetupSteps.svelte'
    import { ProviderSetupController } from '$lib/ai-provider-setup/provider-setup.svelte'
    import { getSetting, setSetting, onSpecificSettingChange, cloudProviderPresets } from '$lib/settings'
    import { pushConfigToBackend } from '$lib/settings/ai-config'
    import type { SecretErrorMessage } from './ai-secret-error'
    import { addToast, dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'

    /**
     * Settings › AI › Provider. The service picker, then the SAME numbered setup steps the
     * onboarding wizard shows (`$lib/ai-provider-setup/`), then the connection status.
     *
     * Everything stateful (key persist, connection check, model cache, provider-switch race
     * guards) lives in the shared controller; this file owns the settings chrome: the
     * service row, the recheck buttons, the Ask Cmdr override note, and mirroring a
     * secret-store failure into a persistent toast.
     */
    interface Props {
        searchQuery: string
        shouldShow: (id: string) => boolean
    }

    const { searchQuery, shouldShow }: Props = $props()

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
        onKeyPersisted: () => void pushConfigToBackend(),
    })

    const unlistenFns: Array<() => void> = []

    onMount(() => {
        controller.setProvider(cloudProviderId)
    })

    const unsubCloudProvider = onSpecificSettingChange('ai.cloudProvider', (newValue) => {
        cloudProviderId = newValue
        // `setProvider` flushes any in-flight typing against the OLD provider's keychain
        // entry before it switches, so a trailing keystroke can't land on the wrong one.
        controller.setProvider(newValue)
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

    const providerSelectItems = $derived<SelectItem[]>(
        cloudProviderPresets.map((preset) => ({ value: preset.id, label: preset.name })),
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

    {#if controller.status === 'checking'}
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
    {:else if controller.status === 'auth-error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{controller.error ?? tString('ai.cloud.authError')}</span
            >
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.status === 'connection-error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{controller.error ?? tString('ai.cloud.connectionError')}</span
            >
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.status === 'error'}
        <div class="connection-status">
            <span class="connection-status-icon connection-status-error">&#x2717;</span>
            <span class="connection-status-text connection-status-error-text"
                >{controller.error ?? tString('ai.cloud.genericError')}</span
            >
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.recheck')}</Button>
        </div>
    {:else if controller.status === 'idle' && controller.hasCheckableConfig}
        <div class="connection-status">
            <Button size="mini" onclick={() => { controller.checkNow(); }}>{tString('ai.cloud.testConnection')}</Button>
        </div>
    {/if}
</SectionCard>

<style>
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
