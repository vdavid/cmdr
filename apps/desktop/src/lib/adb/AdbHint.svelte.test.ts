/**
 * The MTP pane's one line offering USB debugging.
 *
 * The DECISION is pinned next door in `should-show-adb-hint.test.ts`; what this
 * file owes is the wiring: that the line reads the pane it is in, that × hides
 * it AND remembers, and that the "How" link opens Android's own instructions
 * rather than navigating the webview.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import AdbHint from './AdbHint.svelte'

const { stubs } = vi.hoisted(() => ({
  stubs: {
    volumes: [] as unknown[],
    // A `Map`, not an object literal: it carries its own type parameters, so the
    // formatter has no redundant cast to strip and `getSetting` stays typed.
    settings: new Map<string, boolean>(),
    setSetting: vi.fn(),
    openExternalUrl: vi.fn(),
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => stubs.volumes }))
vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => stubs.settings.get(id),
  // Braces, not an expression body: an untyped `vi.fn()` answers `any`, and
  // returning it is what `no-unsafe-return` is about. Nothing reads the answer.
  setSetting: (...args: unknown[]) => {
    stubs.setSetting(...(args as []))
  },
}))
vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: (...args: unknown[]) => {
    stubs.openExternalUrl(...(args as []))
  },
}))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

const MTP_VOLUME_ID = 'mtp-pixel-7:65537'

function phoneOverMtp() {
  return { id: MTP_VOLUME_ID, name: 'Pixel 7', category: 'mobile_device', fsType: 'mtp' }
}

function render(volumeId = MTP_VOLUME_ID): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AdbHint, { target, props: { volumeId } })
  return target
}

describe('AdbHint', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
    stubs.volumes = [phoneOverMtp()]
    stubs.settings = new Map([
      ['fileOperations.adbEnabled', true],
      ['behavior.adbHintDismissed', false],
    ])
  })

  it('offers the fuller way in on a phone reached over MTP', () => {
    const target = render()
    expect(target.querySelector('.adb-hint')).toBeTruthy()
    expect(target.querySelector('.hint-text')?.textContent).toBe('adb.hint.text')
  })

  it('renders nothing on a pane that is not a phone', () => {
    const target = render('root')
    expect(target.querySelector('.adb-hint')).toBeNull()
  })

  it('stays away once the flag is set, so it is said once ever', () => {
    stubs.settings.set('behavior.adbHintDismissed', true)
    expect(render().querySelector('.adb-hint')).toBeNull()
  })

  it('goes at the click and remembers that it went', async () => {
    const target = render()
    const dismiss = target.querySelector('.hint-dismiss') as HTMLButtonElement
    dismiss.click()
    await tick()

    expect(target.querySelector('.adb-hint')).toBeNull()
    expect(stubs.setSetting).toHaveBeenCalledWith('behavior.adbHintDismissed', true)
  })

  // ❗ Through the opener, ❌ never as an `<a>` navigation: Tauri blocks raw link
  // navigation, so the link would silently do nothing.
  it('opens Android’s own instructions in the browser', async () => {
    const target = render()
    const link = target.querySelector('.link-button') as HTMLAnchorElement
    link.click()
    await tick()

    expect(stubs.openExternalUrl).toHaveBeenCalledWith('https://developer.android.com/studio/debug/dev-options')
  })
})
