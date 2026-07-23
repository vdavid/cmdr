/**
 * Tests for `MediaIndexNetworkVolumes.svelte` (the per-network-volume opt-in +
 * "always index" controls in the Image search settings card). Mounts the component
 * with a stubbed network volume and mocked IPC/prefs, asserting the opt-in wiring and
 * running an axe tier-3 audit. All external deps are mocked so the render is
 * deterministic.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { MediaIndexVolumeState } from '$lib/ipc/bindings'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'

const volumes: VolumeInfo[] = [
  { id: 'smb-naspi', name: 'naspi', path: '/Volumes/naspi', category: 'network', isEjectable: true },
  { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
]

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => volumes,
}))

const volumeState = vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>()
vi.mock('$lib/tauri-commands', () => ({
  mediaIndexVolumeState: (v: string) => volumeState(v),
}))

const setNetworkVolumeOptedIn = vi.fn<(id: string, on: boolean) => Promise<void>>()
const setVolumeAlwaysIndexed = vi.fn<(id: string, on: boolean) => Promise<void>>()
let optedIn = false
vi.mock('$lib/media-index/network-volume-prefs', () => ({
  isNetworkVolumeOptedIn: () => optedIn,
  isVolumeAlwaysIndexed: () => false,
  setNetworkVolumeOptedIn: (id: string, on: boolean) => setNetworkVolumeOptedIn(id, on),
  setVolumeAlwaysIndexed: (id: string, on: boolean) => setVolumeAlwaysIndexed(id, on),
}))

vi.mock('$lib/settings', () => ({
  onSpecificSettingChange: () => () => {},
}))

const { default: MediaIndexNetworkVolumes } = await import('./MediaIndexNetworkVolumes.svelte')

function makeState(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 0,
    qualifyingCount: null,
    networkOptIn: optedIn,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: null,
    keptCount: null,
    ...overrides,
  }
}

async function mountAndSettle(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexNetworkVolumes, { target })
  flushSync()
  await vi.advanceTimersByTimeAsync(50)
  await tick()
  vi.useRealTimers()
  return target
}

describe('MediaIndexNetworkVolumes', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    optedIn = false
    volumeState.mockResolvedValue(makeState())
    setNetworkVolumeOptedIn.mockResolvedValue()
    setVolumeAlwaysIndexed.mockResolvedValue()
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('lists only network volumes (not local ones)', async () => {
    const target = await mountAndSettle()
    const names = [...target.querySelectorAll('.net-name')].map((n) => (n.textContent || '').trim())
    expect(names).toEqual(['naspi'])
  })

  it('toggling the opt-in switch calls the persist+apply helper', async () => {
    const target = await mountAndSettle()
    // The hooks sit on the hidden input (the primitive forwards `data-*` there); the
    // styled track is `aria-hidden` decoration.
    const input = target.querySelector('input[data-test="media-net-optin"][data-volume-id="smb-naspi"]')
    expect(input).not.toBeNull()
    ;(input as HTMLElement).click()
    await tick()
    expect(setNetworkVolumeOptedIn).toHaveBeenCalledWith('smb-naspi', true)
  })

  it('the opted-out list has no a11y violations', async () => {
    const target = await mountAndSettle()
    await expectNoA11yViolations(target)
  })

  it('the opted-in list (with always-index row + status) has no a11y violations', async () => {
    optedIn = true
    volumeState.mockResolvedValue(makeState({ networkOptIn: true, enrichedCount: 12 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.net-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
