/**
 * The per-provider step matrix. Pure, so every preset can be walked here rather than
 * mounted: these are the assertions that would have caught Qwen shipping with no links at
 * all (a private id→links map that silently answered empty strings for the one provider
 * nobody added to it).
 */

import { describe, expect, it } from 'vitest'
import { cloudProviderPresets, getCloudProvider, type CloudProviderPreset } from '$lib/settings/cloud-providers'
import { buildSetupPlan, providerHasEditableEndpoint, type SetupStep } from './provider-setup-plan'

function presetOrThrow(id: string): CloudProviderPreset {
  const preset = getCloudProvider(id)
  if (!preset) throw new Error(`no preset for ${id}`)
  return preset
}

function stepIds(preset: CloudProviderPreset): string[] {
  return buildSetupPlan(preset).map((step) => step.id)
}

function linkSteps(preset: CloudProviderPreset): Extract<SetupStep, { kind: 'link' }>[] {
  return buildSetupPlan(preset).filter((step): step is Extract<SetupStep, { kind: 'link' }> => step.kind === 'link')
}

describe('every shipped preset', () => {
  it('resolves a real destination for each of its link steps', () => {
    for (const preset of cloudProviderPresets) {
      for (const step of linkSteps(preset)) {
        expect(step.url, `${preset.id} / ${step.id}`).toMatch(/^https:\/\/\S+$/)
      }
    }
  })

  it('sends every API-key provider somewhere to sign up and somewhere to make a key', () => {
    for (const preset of cloudProviderPresets) {
      if (preset.setup.kind !== 'cloud') continue
      expect(stepIds(preset).slice(0, 2), preset.id).toEqual(['signup', 'createKey'])
    }
  })

  it('ends on the model step, always', () => {
    for (const preset of cloudProviderPresets) {
      expect(stepIds(preset).at(-1), preset.id).toBe('model')
    }
  })

  it('asks for a key exactly when the preset says it needs one', () => {
    for (const preset of cloudProviderPresets) {
      expect(stepIds(preset).includes('apiKey'), preset.id).toBe(preset.requiresApiKey)
    }
  })
})

describe('a cloud provider', () => {
  it('walks sign up → create a key → paste it → pick a model', () => {
    expect(stepIds(presetOrThrow('openai'))).toEqual(['signup', 'createKey', 'apiKey', 'model'])
  })

  /** The bug this file exists for: `qwen` was the one preset missing from the old map. */
  it('gives Qwen the same two links as every other cloud provider', () => {
    const links = linkSteps(presetOrThrow('qwen'))
    expect(links.map((step) => step.id)).toEqual(['signup', 'createKey'])
    expect(links[1]?.url).toContain('modelstudio.console.alibabacloud.com')
  })
})

describe('a local provider', () => {
  it('says install and get a model running, and never asks for an API key', () => {
    expect(stepIds(presetOrThrow('ollama'))).toEqual(['install', 'ollamaModel', 'model'])
    expect(stepIds(presetOrThrow('lm-studio'))).toEqual(['install', 'lmStudioServer', 'model'])
  })

  it('never offers a sign-up step: there is no account to make', () => {
    for (const id of ['ollama', 'lm-studio']) {
      expect(stepIds(presetOrThrow(id)), id).not.toContain('signup')
    }
  })

  it('points at the app itself and at its own get-a-model guide', () => {
    const lmStudio = linkSteps(presetOrThrow('lm-studio'))
    expect(lmStudio[0]?.url).toBe('https://lmstudio.ai/')
    // The old target, `lmstudio.ai/docs/local-server`, 404s (checked 2026-09-09).
    expect(lmStudio[1]?.url).toBe('https://lmstudio.ai/docs/app/api')
  })
})

describe('the endpoint step', () => {
  it('appears only where the user supplies the URL', () => {
    for (const preset of cloudProviderPresets) {
      expect(stepIds(preset).includes('endpoint'), preset.id).toBe(providerHasEditableEndpoint(preset.id))
    }
    expect(providerHasEditableEndpoint('custom')).toBe(true)
    expect(providerHasEditableEndpoint('azure-openai')).toBe(true)
    expect(providerHasEditableEndpoint('openai')).toBe(false)
  })

  it('carries the resource-name hint for Azure and nothing for Custom', () => {
    const azure = buildSetupPlan(presetOrThrow('azure-openai')).find((step) => step.kind === 'endpoint')
    const custom = buildSetupPlan(presetOrThrow('custom')).find((step) => step.kind === 'endpoint')
    expect(azure?.kind === 'endpoint' ? azure.hintKey : null).toBe('onboarding.cloudSetup.hint.azureEndpoint')
    expect(custom?.kind === 'endpoint' ? custom.hintKey : null).toBeNull()
  })
})

describe('the model step', () => {
  it('warns only Azure users that the field wants a deployment name', () => {
    for (const preset of cloudProviderPresets) {
      const step = buildSetupPlan(preset).at(-1)
      const hint = step?.kind === 'model' ? step.hintKey : undefined
      expect(hint, preset.id).toBe(preset.id === 'azure-openai' ? 'onboarding.cloudSetup.hint.azureModel' : null)
    }
  })
})
