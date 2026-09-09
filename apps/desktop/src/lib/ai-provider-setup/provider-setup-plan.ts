/**
 * Turns a cloud-provider preset into the ordered list of setup steps the user walks.
 *
 * Pure and free of Svelte, so the whole per-provider matrix is testable without a DOM.
 * `ProviderSetupSteps.svelte` renders whatever comes back; adding a provider means
 * filling in its `setup` field in `$lib/settings/cloud-providers`, never editing markup.
 *
 * The plan says WHICH steps and in what order; the component owns the wording, because
 * each step's sentence carries an inline link and `<Trans>` snippet parity is only
 * checkable against a literal message key.
 */

import type { CloudProviderPreset } from '$lib/settings/cloud-providers'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * A step that only tells the user to go somewhere. Its sentence wraps `url` in an inline
 * link, so the URL never reaches the message catalog and translators can move the link
 * wherever the sentence needs it.
 */
export interface LinkStep {
  kind: 'link'
  /** Stable id: the `{#each}` key, the component's copy branch, and what tests assert on. */
  id: 'signup' | 'createKey' | 'install' | 'ollamaModel' | 'lmStudioServer'
  url: string
}

/** A step that asks for the endpoint URL (the providers where the user supplies one). */
export interface EndpointStep {
  kind: 'endpoint'
  id: 'endpoint'
  /** Caption under the input, when this provider needs more than the label says. */
  hintKey: MessageKey | null
}

/** A step that asks for the API key, with the live connection status under it. */
export interface ApiKeyStep {
  kind: 'apiKey'
  id: 'apiKey'
}

/** The closing step: which model to use. Present for every provider. */
export interface ModelStep {
  kind: 'model'
  id: 'model'
  hintKey: MessageKey | null
}

export type SetupStep = LinkStep | EndpointStep | ApiKeyStep | ModelStep

/**
 * The providers whose endpoint the user has to fill in. Everything else ships a fixed
 * `baseUrl` on its preset and shows no endpoint step.
 *
 * `custom` is an endpoint by definition. `azure-openai`'s preset URL is a TEMPLATE
 * (`https://{resource-name}.openai.azure.com/openai/v1`) that only reaches a real service
 * once the resource name is substituted, which is why it earns an input plus its own hint.
 */
export function providerHasEditableEndpoint(providerId: string): boolean {
  return providerId === 'custom' || providerId === 'azure-openai'
}

/** Builds the ordered steps for a provider. */
export function buildSetupPlan(preset: CloudProviderPreset): SetupStep[] {
  const steps: SetupStep[] = []

  if (preset.setup.kind === 'cloud') {
    steps.push({ kind: 'link', id: 'signup', url: preset.setup.signupUrl })
    steps.push({ kind: 'link', id: 'createKey', url: preset.setup.apiKeysUrl })
  } else if (preset.setup.kind === 'local') {
    // No account exists for a provider you download and run, so these two REPLACE the
    // sign-up / create-a-key pair rather than sitting alongside it. How you get a model
    // answering differs enough between the two apps that one sentence can't serve both:
    // Ollama pulls from a terminal and serves on its own, LM Studio loads a model in its
    // window and needs its server switched on.
    steps.push({ kind: 'link', id: 'install', url: preset.setup.downloadUrl })
    steps.push({
      kind: 'link',
      id: preset.id === 'lm-studio' ? 'lmStudioServer' : 'ollamaModel',
      url: preset.setup.guideUrl,
    })
  }

  if (providerHasEditableEndpoint(preset.id)) {
    steps.push({
      kind: 'endpoint',
      id: 'endpoint',
      hintKey: preset.id === 'azure-openai' ? 'onboarding.cloudSetup.hint.azureEndpoint' : null,
    })
  }

  if (preset.requiresApiKey) {
    steps.push({ kind: 'apiKey', id: 'apiKey' })
  }

  steps.push({ kind: 'model', id: 'model', hintKey: modelHintKey(preset) })

  return steps
}

function modelHintKey(preset: CloudProviderPreset): MessageKey | null {
  // Azure names a model by the DEPLOYMENT you created, not by the base model id, and the
  // endpoint template invites exactly that mistake. It's the only provider whose model
  // field means something other than "the model's name", so it's the only hint here.
  return preset.id === 'azure-openai' ? 'onboarding.cloudSetup.hint.azureModel' : null
}
