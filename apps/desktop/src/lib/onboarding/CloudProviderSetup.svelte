<script lang="ts">
    import { onDestroy, untrack } from 'svelte'
    import ProviderSetupSteps from '$lib/ai-provider-setup/ProviderSetupSteps.svelte'
    import { ProviderSetupController } from '$lib/ai-provider-setup/provider-setup.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    /**
     * Per-provider tutorial in the onboarding wizard's step 2 right column: a provider
     * header plus the shared numbered steps, with a quiet status line underneath.
     *
     * The steps, the API-key persist, and the connection check all live in
     * `$lib/ai-provider-setup/`, shared with Settings › AI › Provider. Only the header and
     * the status line are the wizard's own, and the status line is deliberately quieter
     * than Settings': the wizard never blocks advance on a key, so a failed check is
     * feedback, not something to act on right now.
     *
     * Provider switching is owned by the parent (`StepAi.svelte`); on a `providerId` change
     * the controller reloads from the store and the secret keychain. Keys typed for earlier
     * providers stay in the secret store, so hopping back doesn't mean re-entering them.
     */
    interface Props {
        providerId: string
    }

    const { providerId }: Props = $props()

    const controller = new ProviderSetupController({ logScope: 'onboarding-ai-setup' })

    // Point the controller at whichever provider the parent has selected. `untrack` is
    // load-bearing: `setProvider` both reads and writes the controller's `$state`, so a
    // tracked call would re-run itself the moment a check or a keychain read lands, and
    // reset the state it just produced.
    $effect(() => {
        const id = providerId
        untrack(() => { controller.setProvider(id); })
    })

    onDestroy(() => { controller.destroy(); })

    const preset = $derived(controller.preset)
</script>

<div class="setup-panel" data-provider-id={controller.providerId}>
    {#if preset}
        <header class="provider-header">
            <h3 class="provider-title">{tString('onboarding.cloudSetup.title', { provider: preset.name })}</h3>
            {#if preset.description}
                <p class="provider-description">{preset.description}</p>
            {/if}
        </header>

        <ProviderSetupSteps {controller} idPrefix="onboarding-cloud" />

        {#if controller.secretError}
            <p class="status status-error" role="alert">{controller.secretError.title}</p>
        {:else if controller.status === 'checking'}
            <p class="status status-checking">{tString('onboarding.cloudSetup.status.checking')}</p>
        {:else if controller.status === 'auth-error'}
            <p class="status status-error">
                {controller.error ?? tString('onboarding.cloudSetup.status.authError')}
            </p>
        {:else if controller.status === 'connection-error'}
            <p class="status status-error">
                {controller.error ?? tString('onboarding.cloudSetup.status.connectionError')}
            </p>
        {:else if controller.status === 'error'}
            <p class="status status-error">
                {controller.error ?? tString('onboarding.cloudSetup.status.genericError')}
            </p>
        {:else if controller.isConnected}
            <p class="status status-ok">{tString('onboarding.cloudSetup.status.connected')}</p>
        {/if}
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
    }

    .status {
        margin: 0;
        font-size: var(--font-size-sm);
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
