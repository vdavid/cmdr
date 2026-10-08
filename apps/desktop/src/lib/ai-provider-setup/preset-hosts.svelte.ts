/**
 * Which cloud AI service presets the organization's policy refuses (`AllowedCloudAIHosts`), for
 * the two service pickers: Settings › AI › Provider's dropdown and the onboarding wizard's list. A
 * refused preset stays listed, disabled with the reason, so the person sees what IT ruled out.
 *
 * The backend judges each preset's URL (`cloud_ai_host_verdicts`); nothing here reads the host
 * list. A preset with an editable endpoint (custom, Azure) isn't judged by its placeholder: the
 * person's own URL is checked once entered (`ProviderSetupController`).
 */

import { untrack } from 'svelte'
import { cloudProviderPresets } from '$lib/settings/cloud-providers'
import { cloudAiHostVerdicts } from '$lib/tauri-commands'
import { getManagedPolicyView } from '$lib/managed-policy/managed-policy.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { providerHasEditableEndpoint } from './provider-setup-plan'

const log = getAppLogger('ai-provider-setup')

export class PresetHostVerdicts {
  #refused = $state<readonly string[]>([])
  /** Bumped per ask, so an older answer landing late can't overwrite a newer one. */
  #generation = 0

  /** Whether the policy refuses this preset's endpoint. Reactive. */
  isRefused(id: string): boolean {
    return this.#refused.includes(id)
  }

  /** Asks the backend about every fixed-endpoint preset. A failed ask refuses nothing. */
  async refresh(): Promise<void> {
    const generation = ++this.#generation
    const judged = cloudProviderPresets.filter((preset) => !providerHasEditableEndpoint(preset.id))
    let refused: string[] = []
    try {
      const verdicts = await cloudAiHostVerdicts(judged.map((preset) => preset.baseUrl))
      refused = judged.filter((_, index) => (verdicts[index] ?? null) !== null).map((preset) => preset.id)
    } catch (e) {
      log.debug("Couldn't ask the policy about the service presets, so none reads as refused: {error}", {
        error: e,
      })
    }
    if (generation === this.#generation) this.#refused = refused
  }
}

/**
 * Verdicts that follow the policy: asked on mount and again whenever the policy changes. Call
 * during a component's initialization (it registers an `$effect`).
 */
export function followPresetHostVerdicts(): PresetHostVerdicts {
  const verdicts = new PresetHostVerdicts()
  $effect(() => {
    // The view object is replaced on every change, so reading it is the subscription.
    getManagedPolicyView()
    untrack(() => void verdicts.refresh())
  })
  return verdicts
}
