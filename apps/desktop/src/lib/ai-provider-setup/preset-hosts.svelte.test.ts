/**
 * Which service presets the organization's policy refuses, as the two pickers read it: asked of
 * the backend per preset URL, never worked out here.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'
import type { ManagedAiRefusal } from '$lib/ipc/bindings'
import { cloudProviderPresets } from '$lib/settings/cloud-providers'
import { PresetHostVerdicts } from './preset-hosts.svelte'

const cloudAiHostVerdicts = vi.fn<(baseUrls: string[]) => Promise<(ManagedAiRefusal | null)[]>>()

vi.mock('$lib/tauri-commands', () => ({
  cloudAiHostVerdicts: (baseUrls: string[]) => cloudAiHostVerdicts(baseUrls),
}))

const openAiUrl = cloudProviderPresets.find((preset) => preset.id === 'openai')?.baseUrl ?? ''

beforeEach(() => {
  cloudAiHostVerdicts.mockReset()
})

describe('PresetHostVerdicts', () => {
  it('marks exactly the presets whose endpoint the backend refuses', async () => {
    cloudAiHostVerdicts.mockImplementation((urls) =>
      Promise.resolve(urls.map((url) => (url === openAiUrl ? ('hostNotAllowed' as const) : null))),
    )
    const verdicts = new PresetHostVerdicts()
    expect(verdicts.isRefused('openai')).toBe(false)

    await verdicts.refresh()

    expect(verdicts.isRefused('openai')).toBe(true)
    expect(verdicts.isRefused('anthropic')).toBe(false)
  })

  it("never judges a preset whose endpoint is the person's own: that URL is checked once entered", async () => {
    cloudAiHostVerdicts.mockImplementation((urls) => Promise.resolve(urls.map(() => 'hostNotAllowed' as const)))
    const verdicts = new PresetHostVerdicts()
    await verdicts.refresh()

    const asked = cloudAiHostVerdicts.mock.calls[0]?.[0] ?? []
    const placeholder = cloudProviderPresets.find((preset) => preset.id === 'azure-openai')?.baseUrl
    expect(asked).not.toContain(placeholder)
    expect(verdicts.isRefused('custom')).toBe(false)
    expect(verdicts.isRefused('azure-openai')).toBe(false)
    expect(verdicts.isRefused('openai')).toBe(true)
  })

  it('refuses nothing when the backend can’t be asked: the backend still refuses the request', async () => {
    cloudAiHostVerdicts.mockRejectedValue(new Error('no backend'))
    const verdicts = new PresetHostVerdicts()
    await verdicts.refresh()
    expect(verdicts.isRefused('openai')).toBe(false)
  })

  it('keeps the newest answer when two asks overlap', async () => {
    let releaseFirst: ((verdicts: (ManagedAiRefusal | null)[]) => void) | undefined
    cloudAiHostVerdicts.mockImplementationOnce(
      () =>
        new Promise<(ManagedAiRefusal | null)[]>((resolve) => {
          releaseFirst = resolve
        }),
    )
    cloudAiHostVerdicts.mockImplementation((urls) => Promise.resolve(urls.map(() => null)))
    const verdicts = new PresetHostVerdicts()
    const first = verdicts.refresh()
    await verdicts.refresh()
    releaseFirst?.(cloudProviderPresets.map(() => 'hostNotAllowed' as const))
    await first
    expect(verdicts.isRefused('openai')).toBe(false)
  })
})
