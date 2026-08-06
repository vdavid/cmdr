/**
 * The startup gates decide what a user SEES on launch: wizard or explorer, the
 * one-time upgrade nudge, and the "What's new" popup. Every branch below was
 * previously reachable only by mounting the whole app shell.
 *
 * The six-row onboarding truth table is the load-bearing part: getting a row
 * wrong either re-prompts someone who already answered, or drops a first-run
 * user straight into an explorer with no disk access.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FullDiskAccessChoice, Settings } from '$lib/settings-store'

const mocks = vi.hoisted(() => ({
  isForceOnboarding: vi.fn(),
  checkFullDiskAccess: vi.fn(),
  openWizard: vi.fn(),
  runWhatsNewStartupTrigger: vi.fn(),
  loadSettings: vi.fn(),
  saveSettings: vi.fn(),
  getSetting: vi.fn(),
  setSetting: vi.fn(),
  getAppMode: vi.fn(),
  notifyOnboardingComplete: vi.fn(),
  isMacOS: vi.fn(),
  addToast: vi.fn(),
  warn: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  isForceOnboarding: mocks.isForceOnboarding,
  checkFullDiskAccess: mocks.checkFullDiskAccess,
}))
vi.mock('$lib/onboarding/onboarding-state.svelte', () => ({ openWizard: mocks.openWizard }))
vi.mock('$lib/whats-new/whats-new-trigger.svelte', () => ({
  runWhatsNewStartupTrigger: mocks.runWhatsNewStartupTrigger,
}))
vi.mock('$lib/settings-store', () => ({ loadSettings: mocks.loadSettings, saveSettings: mocks.saveSettings }))
vi.mock('$lib/settings', () => ({ getSetting: mocks.getSetting, setSetting: mocks.setSetting }))
// `isE2eRun` mirrors the real helper: a capture run counts as an E2E run.
vi.mock('$lib/app-mode', () => ({
  getAppMode: mocks.getAppMode,
  isE2eRun: () => mocks.getAppMode() === 'e2e' || mocks.getAppMode() === 'capture',
}))
vi.mock('$lib/updates/updater.svelte', () => ({ notifyOnboardingComplete: mocks.notifyOnboardingComplete }))
vi.mock('$lib/shortcuts/key-capture', () => ({ isMacOS: mocks.isMacOS }))
vi.mock('$lib/ui/toast', () => ({ addToast: mocks.addToast }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: mocks.warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import {
  maybeFireUpgradeNudge,
  maybeRunWhatsNew,
  openOnboardingFromMenuOrPalette,
  resolveOnboardingMount,
  type StartupGatesContext,
} from './startup-gates'

let onboardingVisible: boolean
let appShown: boolean
let otherModalOpen: boolean
let ctx: StartupGatesContext

/** Only the two fields the gates read; the rest of `Settings` is irrelevant here. */
function settings(fullDiskAccessChoice: FullDiskAccessChoice, isOnboarded: boolean): Settings {
  return { fullDiskAccessChoice, isOnboarded } as Settings
}

beforeEach(() => {
  vi.clearAllMocks()
  onboardingVisible = false
  appShown = false
  otherModalOpen = false
  ctx = {
    setOnboardingVisible: (visible: boolean) => {
      onboardingVisible = visible
    },
    isOnboardingVisible: () => onboardingVisible,
    showApp: () => {
      appShown = true
    },
    isOtherStartupModalOpen: () => otherModalOpen,
  }
  mocks.isForceOnboarding.mockResolvedValue(false)
  mocks.checkFullDiskAccess.mockResolvedValue(false)
  mocks.loadSettings.mockResolvedValue(settings('notAskedYet', false))
  mocks.saveSettings.mockResolvedValue(true)
  mocks.notifyOnboardingComplete.mockResolvedValue(undefined)
  mocks.runWhatsNewStartupTrigger.mockResolvedValue(undefined)
  mocks.getAppMode.mockReturnValue('prod')
  mocks.getSetting.mockReturnValue(false)
  mocks.isMacOS.mockReturnValue(true)
})

describe('resolveOnboardingMount', () => {
  it('forces the wizard when CMDR_FORCE_ONBOARDING is set, whatever the settings say', async () => {
    mocks.isForceOnboarding.mockResolvedValue(true)
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('allow', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.openWizard).toHaveBeenCalledWith('force', {
      fullDiskAccessChoice: 'allow',
      isOnboarded: true,
      hasFda: true,
    })
    expect(onboardingVisible).toBe(true)
    expect(appShown).toBe(true)
    // The nudge points at a menu item the forced wizard is already showing.
    expect(mocks.addToast).not.toHaveBeenCalled()
  })

  it('treats a failing force probe as "not forced"', async () => {
    mocks.isForceOnboarding.mockRejectedValue(new Error('no backend'))
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('allow', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.openWizard).not.toHaveBeenCalled()
    expect(appShown).toBe(true)
  })

  it('skips the wizard when FDA is granted and mirrors a diverged setting', async () => {
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('deny', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.saveSettings).toHaveBeenCalledWith({ fullDiskAccessChoice: 'allow' })
    expect(mocks.notifyOnboardingComplete).not.toHaveBeenCalled()
    expect(onboardingVisible).toBe(false)
    expect(appShown).toBe(true)
  })

  it('warns instead of throwing when the mirror write fails', async () => {
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('deny', true))
    mocks.saveSettings.mockResolvedValue(false)

    await resolveOnboardingMount(ctx)

    expect(mocks.warn).toHaveBeenCalledOnce()
    expect(appShown).toBe(true)
  })

  it('marks a pre-wizard FDA user onboarded', async () => {
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('allow', false))

    await resolveOnboardingMount(ctx)

    expect(mocks.saveSettings).not.toHaveBeenCalled()
    expect(mocks.notifyOnboardingComplete).toHaveBeenCalledOnce()
    expect(appShown).toBe(true)
  })

  it('does not re-prompt someone who denied and finished onboarding', async () => {
    mocks.loadSettings.mockResolvedValue(settings('deny', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.openWizard).not.toHaveBeenCalled()
    expect(onboardingVisible).toBe(false)
    expect(appShown).toBe(true)
  })

  it.each([
    ['notAskedYet', false],
    ['allow', false],
    ['allow', true],
    ['deny', false],
  ] as const)('routes fullDiskAccessChoice=%s / isOnboarded=%s through the wizard', async (choice, onboarded) => {
    mocks.loadSettings.mockResolvedValue(settings(choice, onboarded))

    await resolveOnboardingMount(ctx)

    expect(mocks.openWizard).toHaveBeenCalledWith('first-launch', {
      fullDiskAccessChoice: choice,
      isOnboarded: onboarded,
      hasFda: false,
    })
    expect(onboardingVisible).toBe(true)
    expect(appShown).toBe(true)
  })

  it('reveals the app shell on every branch, so no launch can strand the user on a blank window', async () => {
    for (const choice of ['notAskedYet', 'allow', 'deny'] as const) {
      for (const onboarded of [false, true]) {
        for (const hasFda of [false, true]) {
          appShown = false
          mocks.checkFullDiskAccess.mockResolvedValue(hasFda)
          mocks.loadSettings.mockResolvedValue(settings(choice, onboarded))

          await resolveOnboardingMount(ctx)

          expect(appShown, `${choice} / onboarded=${String(onboarded)} / hasFda=${String(hasFda)}`).toBe(true)
        }
      }
    }
  })
})

describe('maybeFireUpgradeNudge', () => {
  it('fires the macOS toast once and records it', () => {
    maybeFireUpgradeNudge()

    expect(mocks.addToast).toHaveBeenCalledWith('main.upgradeNudge.mac', { level: 'info' })
    expect(mocks.setSetting).toHaveBeenCalledWith('onboarding.upgradeNudgeShown', true)
  })

  it('uses the palette wording off macOS', () => {
    mocks.isMacOS.mockReturnValue(false)

    maybeFireUpgradeNudge()

    expect(mocks.addToast).toHaveBeenCalledWith('main.upgradeNudge.other', { level: 'info' })
  })

  it('stays quiet once it has already fired', () => {
    mocks.getSetting.mockReturnValue(true)

    maybeFireUpgradeNudge()

    expect(mocks.addToast).not.toHaveBeenCalled()
    expect(mocks.setSetting).not.toHaveBeenCalled()
  })

  it('stays quiet under E2E mode so it cannot leak into the first spec', () => {
    mocks.getAppMode.mockReturnValue('e2e')

    maybeFireUpgradeNudge()

    expect(mocks.addToast).not.toHaveBeenCalled()
    expect(mocks.getSetting).not.toHaveBeenCalled()
  })

  it('fires from the FDA-granted mount branch', async () => {
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('allow', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.addToast).toHaveBeenCalledOnce()
  })

  it('fires from the denied-and-onboarded mount branch', async () => {
    mocks.loadSettings.mockResolvedValue(settings('deny', true))

    await resolveOnboardingMount(ctx)

    expect(mocks.addToast).toHaveBeenCalledOnce()
  })
})

describe('maybeRunWhatsNew', () => {
  it('hands the live modal state to the trigger', async () => {
    mocks.loadSettings.mockResolvedValue(settings('allow', true))
    onboardingVisible = true
    otherModalOpen = true

    await maybeRunWhatsNew(ctx)

    expect(mocks.runWhatsNewStartupTrigger).toHaveBeenCalledWith({
      onboarded: true,
      onboardingShowing: true,
      otherStartupModalOpen: true,
    })
  })

  it('reads the wizard flag live, not as a snapshot taken when the gates were wired', async () => {
    await maybeRunWhatsNew(ctx)
    onboardingVisible = true
    await maybeRunWhatsNew(ctx)

    expect(mocks.runWhatsNewStartupTrigger).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ onboardingShowing: false }),
    )
    expect(mocks.runWhatsNewStartupTrigger).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ onboardingShowing: true }),
    )
  })

  it('skips the boot run under E2E mode', async () => {
    mocks.getAppMode.mockReturnValue('e2e')

    await maybeRunWhatsNew(ctx)

    expect(mocks.runWhatsNewStartupTrigger).not.toHaveBeenCalled()
  })

  it('runs under E2E mode when forced by the rerun hook', async () => {
    mocks.getAppMode.mockReturnValue('e2e')

    await maybeRunWhatsNew(ctx, true)

    expect(mocks.runWhatsNewStartupTrigger).toHaveBeenCalledOnce()
  })
})

describe('openOnboardingFromMenuOrPalette', () => {
  it.each(['menu', 'palette'] as const)('opens the wizard from the %s', async (source) => {
    mocks.checkFullDiskAccess.mockResolvedValue(true)
    mocks.loadSettings.mockResolvedValue(settings('allow', true))

    await openOnboardingFromMenuOrPalette(ctx, source)

    expect(mocks.openWizard).toHaveBeenCalledWith(source, {
      fullDiskAccessChoice: 'allow',
      isOnboarded: true,
      hasFda: true,
    })
    expect(onboardingVisible).toBe(true)
    // Re-entry only opens the wizard; the shell is already up by then.
    expect(appShown).toBe(false)
  })

  it('no-ops when the wizard is already open', async () => {
    onboardingVisible = true

    await openOnboardingFromMenuOrPalette(ctx, 'menu')

    expect(mocks.openWizard).not.toHaveBeenCalled()
    expect(mocks.loadSettings).not.toHaveBeenCalled()
  })
})
