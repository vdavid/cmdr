<script lang="ts">
    import type { Snippet } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Combobox, { type ComboboxItem } from '$lib/ui/Combobox.svelte'
    import SettingPasswordInput from '$lib/settings/components/SettingPasswordInput.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { buildSetupPlan } from './provider-setup-plan'
    import type { ProviderSetupController } from './provider-setup.svelte'

    /**
     * The numbered "how to set this provider up" list, shared by the onboarding wizard's AI
     * step and Settings › AI › Provider. Each step carries a marker that starts as its
     * number and flips to a checkmark once the step is satisfied.
     *
     * Which steps appear, and in which order, comes from `buildSetupPlan(preset)`; this file
     * owns only the wording and the controls. Copy branches on a literal message key rather
     * than one looked up from the plan, so `desktop-i18n-trans-snippet-parity` can still
     * match every `<tag>` to its snippet.
     *
     * The connection status is NOT rendered here: the two surfaces show it differently (the
     * wizard as a quiet line, Settings as a row with a recheck button), so each owns that
     * block and this one stays presentation the two genuinely share.
     */
    interface Props {
        controller: ProviderSetupController
        /** Prefixes the DOM ids this renders, so two surfaces can't collide. */
        idPrefix: string
    }

    const { controller, idPrefix }: Props = $props()
    const log = getAppLogger('ai-provider-setup')

    const preset = $derived(controller.preset)
    const steps = $derived(preset ? buildSetupPlan(preset) : [])
    const modelComboboxItems = $derived<ComboboxItem[]>(controller.models.map((m) => ({ value: m, label: m })))
    const modelPlaceholder = $derived(
        preset?.defaultModel
            ? tString('onboarding.cloudSetup.modelPlaceholderExample', { model: preset.defaultModel })
            : tString('onboarding.cloudSetup.modelPlaceholder'),
    )
    const apiKeyPlaceholder = $derived(
        controller.keyIsSet
            ? tString('onboarding.cloudSetup.apiKeyPlaceholder.saved')
            : controller.providerId === 'openai'
              ? tString('onboarding.cloudSetup.apiKeyPlaceholder.openai')
              : controller.providerId === 'anthropic'
                ? tString('onboarding.cloudSetup.apiKeyPlaceholder.anthropic')
                : tString('onboarding.cloudSetup.apiKeyPlaceholder.generic'),
    )

    function openUrl(url: string): void {
        if (!url) return
        void openExternalUrl(url).catch((error: unknown) => {
            log.warn('openExternalUrl({url}) failed: {error}', { url, error })
        })
    }

    /** A step's marker shows a checkmark once the thing it asks for is in place. */
    function isDone(stepId: string): boolean {
        switch (stepId) {
            case 'endpoint':
                return controller.baseUrl.trim() !== ''
            case 'apiKey':
                return controller.isConnected
            case 'model':
                return controller.model.trim() !== ''
            // Link steps can't be verified from here, so they read as done: they're
            // directions, not something the app can confirm.
            default:
                return true
        }
    }
</script>

<ol class="setup-steps">
    {#each steps as step (step.id)}
        <li class="setup-step" class:done={isDone(step.id)}>
            <span class="step-marker" aria-hidden="true">
                {#if isDone(step.id)}
                    <Icon name="check" size={14} />
                {/if}
            </span>
            <div class="step-body">
                {#if step.kind === 'link'}
                    {@const url = step.url}
                    {#snippet stepLink(children: Snippet)}<LinkButton
                            href={url}
                            target="_blank"
                            rel="noopener noreferrer"
                            onclick={(event: MouseEvent) => {
                                event.preventDefault()
                                openUrl(url)
                            }}>{@render children()}</LinkButton
                        >{/snippet}
                    <span class="step-label">
                        {#if step.id === 'signup'}
                            <Trans
                                key="onboarding.cloudSetup.step.signup"
                                snippets={{ signupLink: stepLink }}
                                params={{ provider: preset?.name ?? '' }}
                            />
                        {:else if step.id === 'createKey'}
                            <Trans key="onboarding.cloudSetup.step.createKey" snippets={{ keyLink: stepLink }} />
                        {:else if step.id === 'install'}
                            <Trans
                                key="onboarding.cloudSetup.step.install"
                                snippets={{ installLink: stepLink }}
                                params={{ provider: preset?.name ?? '' }}
                            />
                        {:else if step.id === 'ollamaModel'}
                            <Trans key="onboarding.cloudSetup.step.ollamaModel" snippets={{ libraryLink: stepLink }} />
                        {:else}
                            <Trans key="onboarding.cloudSetup.step.lmStudioServer" snippets={{ serverLink: stepLink }} />
                        {/if}
                    </span>
                {:else if step.kind === 'endpoint'}
                    <label class="step-label" for={`${idPrefix}-base-url`}
                        >{tString('onboarding.cloudSetup.step.endpoint')}</label
                    >
                    <TextInput
                        id={`${idPrefix}-base-url`}
                        type="url"
                        value={controller.baseUrl}
                        oninput={(event: Event) => {
                            controller.saveBaseUrl((event.currentTarget as HTMLInputElement).value)
                        }}
                        placeholder={tString('onboarding.cloudSetup.step.endpointPlaceholder')}
                        autocomplete="off"
                        spellcheck={false}
                    />
                    {@const endpointHintKey = step.hintKey}
                    {#if endpointHintKey}
                        <p class="step-hint">{tString(endpointHintKey)}</p>
                    {/if}
                {:else if step.kind === 'apiKey'}
                    <!-- A span, not a label: `SettingPasswordInput` owns its input's id, so a
                         `for` here would point at nothing. The field carries its own aria-label. -->
                    <span class="step-label">{tString('onboarding.cloudSetup.step.pasteKey')}</span>
                    <SettingPasswordInput
                        id="ai.cloudProviderConfigs"
                        placeholder={apiKeyPlaceholder}
                        ariaLabel={tString('onboarding.cloudSetup.apiKeyAria')}
                        value={controller.apiKey}
                        onchange={(value: string) => { controller.handleApiKeyChange(value); }}
                    />
                {:else if step.kind === 'model'}
                    <span class="step-label">{tString('onboarding.cloudSetup.step.pickModel')}</span>
                    <Combobox
                        items={modelComboboxItems}
                        inputValue={controller.model}
                        onInputValueChange={(value: string) => { controller.saveModel(value); }}
                        loading={controller.isChecking}
                        placeholder={modelPlaceholder}
                        ariaLabel={tString('onboarding.cloudSetup.modelAria')}
                    />
                    {@const modelHintKey = step.hintKey}
                    {#if modelHintKey}
                        <p class="step-hint">{tString(modelHintKey)}</p>
                    {/if}
                {/if}
            </div>
        </li>
    {/each}
</ol>

<style>
    .setup-steps {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
        counter-reset: setup-step;
    }

    .setup-step {
        display: grid;
        grid-template-columns: 24px 1fr;
        gap: var(--spacing-sm);
        align-items: start;
        counter-increment: setup-step;
    }

    .step-marker {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        border-radius: var(--radius-full);
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        font-weight: 600;
        line-height: var(--font-line-height-flat);
    }

    .step-marker::before {
        content: counter(setup-step);
    }

    .setup-step.done .step-marker {
        /* Tinted bg + check icon in the allow color. Keep contrast over neutral text
           bg primary; the icon is decorative (the step status is also conveyed by the
           bolder body text), so we don't need a 4.5:1 token-pair. */
        background: transparent;
        color: var(--color-allow);
        border: 1px solid var(--color-allow);
    }

    .setup-step.done .step-marker::before {
        content: '';
    }

    .step-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .step-label {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .step-hint {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
