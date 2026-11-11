/**
 * The badge's visibility rules. Each one exists because the wrong answer is either a scary
 * label on a machine that's fine, or silence on the machine that needs the hint.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { withCapabilities } from '#lib/platform/test-platform.js'
import { mount, tick, unmount } from 'svelte'

const { checkFullDiskAccessQuiet } = vi.hoisted(() => ({
  checkFullDiskAccessQuiet: vi.fn<() => Promise<boolean>>(() => Promise.resolve(false)),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  checkFullDiskAccessQuiet: () => checkFullDiskAccessQuiet(),
}))
vi.hoisted(() => vi.stubGlobal('__CMDR_CAPABILITIES__', { fullDiskAccess: true }))

import { markDialogOpen, markDialogClosed, _resetOpenDialogsForTesting } from '#lib/ui/open-dialogs.svelte.js'
import { refreshFdaStatus, _resetFdaStatusForTests } from './fda-status.svelte'
import { resetForTesting, setCurrentStep } from './onboarding-state.svelte'
import FdaBadge from './FdaBadge.svelte'

let mounted: { target: HTMLElement; instance: Record<string, unknown> } | undefined

async function renderBadge(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(FdaBadge, { target, props: { onOpenOnboarding: () => {} } })
  mounted = { target, instance }
  await tick()
  return target
}

/** Puts the wizard on screen at the given step, the way the real wizard announces itself. */
function showWizardAt(step: 1 | 2): void {
  setCurrentStep(step)
  markDialogOpen('onboarding')
}

describe('FdaBadge', () => {
  beforeEach(() => {
    _resetFdaStatusForTests()
    _resetOpenDialogsForTesting()
    resetForTesting()
    withCapabilities('fullDiskAccess')
    checkFullDiskAccessQuiet.mockResolvedValue(false)
  })

  afterEach(async () => {
    if (mounted) {
      await unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
  })

  it('stays hidden before the first probe answers', async () => {
    // `null` must not render as "no access", or every launch flashes the badge at people
    // who granted it long ago.
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).toBeNull()
  })

  it('shows once the probe says the grant is missing', async () => {
    await refreshFdaStatus()
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).not.toBeNull()
  })

  it('stays hidden when the grant is there', async () => {
    checkFullDiskAccessQuiet.mockResolvedValue(true)
    await refreshFdaStatus()
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).toBeNull()
  })

  it('stays hidden where the OS has no such permission', async () => {
    withCapabilities()
    await refreshFdaStatus()
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).toBeNull()
  })

  it('stays hidden while the wizard is already on the FDA step', async () => {
    await refreshFdaStatus()
    showWizardAt(1)
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).toBeNull()
  })

  it('shows behind the wizard on a later step, which is not where it points', async () => {
    await refreshFdaStatus()
    showWizardAt(2)
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).not.toBeNull()
  })

  it('shows again once the wizard closes on step 1, whose cursor outlives the sheet', async () => {
    // The wizard closes by being unmounted, so `currentStep` stays at 1 afterwards. Reading
    // the step alone would leave the badge hidden for the rest of the session.
    await refreshFdaStatus()
    showWizardAt(1)
    markDialogClosed('onboarding')
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).not.toBeNull()
  })

  it('keeps the last answer when the probe throws, rather than inventing a missing grant', async () => {
    checkFullDiskAccessQuiet.mockResolvedValue(true)
    await refreshFdaStatus()
    checkFullDiskAccessQuiet.mockRejectedValue(new Error('IPC down'))
    await refreshFdaStatus()
    const target = await renderBadge()
    expect(target.querySelector('.fda-badge')).toBeNull()
  })
})
