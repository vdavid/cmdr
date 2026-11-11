import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { withPlatform } from '#lib/platform/test-platform.js'

const mocks = vi.hoisted(() => ({ getStatus: vi.fn(), setEnabled: vi.fn() }))

vi.mock('#lib/tauri-commands/index.js', () => ({
  getFinderExtensionStatus: mocks.getStatus,
  setFinderExtensionEnabled: mocks.setEnabled,
}))

import FinderExtensionCard from './FinderExtensionCard.svelte'

async function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(FinderExtensionCard, { target, props: { searchQuery: '' } })
  await tick()
  await Promise.resolve()
  await tick()
  return { target, cleanup: () => void unmount(component) }
}

function switchOf(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('[data-test="finder-extension-switch"] input, input[data-test="finder-extension-switch"]')
}

describe('FinderExtensionCard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    withPlatform('macos')
  })

  it('renders nothing off macOS, where there is no extension to ask about', async () => {
    withPlatform('linux')
    const { target, cleanup } = await render()
    expect(mocks.getStatus).not.toHaveBeenCalled()
    expect(target.textContent.trim()).toBe('')
    cleanup()
  })

  it('renders nothing when the backend does not answer', async () => {
    mocks.getStatus.mockResolvedValue(null)
    const { target, cleanup } = await render()
    expect(target.textContent.trim()).toBe('')
    cleanup()
  })

  it('shows the reason when this copy may not switch it on', async () => {
    mocks.getStatus.mockResolvedValue({ enabled: false, blockedBy: 'notInApplications' })
    const { target, cleanup } = await render()
    expect(target.querySelector('[data-test="finder-extension-blocked"]')).not.toBeNull()
    cleanup()
  })

  it('shows no reason when the switch is free', async () => {
    mocks.getStatus.mockResolvedValue({ enabled: true, blockedBy: null })
    const { target, cleanup } = await render()
    expect(target.querySelector('[data-test="finder-extension-blocked"]')).toBeNull()
    expect(switchOf(target) ?? target.querySelector('[data-test="finder-extension-switch"]')).not.toBeNull()
    cleanup()
  })
})
