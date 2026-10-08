/**
 * A setting the organization manages, as the rows render it: every row primitive and `SettingRow`
 * pick the lock up themselves (no section has to remember), and a section holding a managed row
 * says so at its top. The backend's `locked_settings` decides which ids; here it's a stub.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { disabledNoteId } from '$lib/settings/settings-window'

const policy = vi.hoisted(() => ({
  locked: new Set<string>(),
  narrowed: new Set<string>(),
}))

vi.mock('$lib/managed-policy/managed-policy.svelte', () => ({
  isSettingLocked: (id: string) => policy.locked.has(id),
  isSettingManaged: (id: string) => policy.locked.has(id) || policy.narrowed.has(id),
}))

const settingsApi = vi.hoisted(() => ({
  getSetting: vi.fn((): unknown => true),
  setSetting: vi.fn(),
  getSettingDefinition: vi.fn(() => ({ label: 'Share usage stats', description: '' })),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => true),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...settingsApi,
}))
vi.mock('$lib/settings/settings-store', () => settingsApi)

import ManagedLockFixture from '../../../../test/fixtures/managed-setting-section-fixture.svelte'

const ROW_NOTE = 'Your organization manages this setting.'
const SECTION_NOTE = 'Your organization manages some of these settings.'

function render(props: { disabled?: boolean; disabledNote?: string; disabledReason?: string } = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ManagedLockFixture, { target, props: { id: 'analytics.enabled', ...props } })
  return target
}

beforeEach(() => {
  policy.locked.clear()
  policy.narrowed.clear()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('a managed setting row', () => {
  it('renders disabled with the managed note, described by it, and offers no reset', async () => {
    policy.locked.add('analytics.enabled')
    const target = render()
    await tick()

    const control = target.querySelector<HTMLInputElement>('input[role="switch"]')
    const note = document.getElementById(disabledNoteId('analytics.enabled'))
    expect(control?.disabled).toBe(true)
    expect(note?.textContent).toContain(ROW_NOTE)
    expect(control?.getAttribute('aria-describedby')).toBe(disabledNoteId('analytics.enabled'))
    expect(target.querySelector('.reset-button')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows only the managed note when the section passes a reason of its own', async () => {
    policy.locked.add('analytics.enabled')
    const target = render({ disabled: true, disabledNote: 'Needs Apple Silicon.', disabledReason: 'Mac only' })
    await tick()

    expect(target.textContent).toContain(ROW_NOTE)
    expect(target.textContent).not.toContain('Needs Apple Silicon.')
    expect(target.textContent).not.toContain('Mac only')
  })

  it('leaves an unmanaged row alone', async () => {
    const target = render()
    await tick()

    expect(target.querySelector<HTMLInputElement>('input[role="switch"]')?.disabled).toBe(false)
    expect(target.textContent).not.toContain(ROW_NOTE)
    expect(target.querySelector('.reset-button')).not.toBeNull()
  })

  it('keeps a narrowed setting usable: its own control rules out the values', async () => {
    policy.narrowed.add('analytics.enabled')
    const target = render()
    await tick()

    expect(target.querySelector<HTMLInputElement>('input[role="switch"]')?.disabled).toBe(false)
    expect(target.textContent).not.toContain(ROW_NOTE)
  })
})

describe('a section holding a managed row', () => {
  it('says so at its top, before the controls', async () => {
    policy.locked.add('analytics.enabled')
    const target = render()
    await tick()

    const line = target.querySelector('.managed-section-note')
    expect(line?.textContent).toContain(SECTION_NOTE)
    const firstControl = target.querySelector('input[role="switch"]')
    expect(
      line && firstControl ? line.compareDocumentPosition(firstControl) & Node.DOCUMENT_POSITION_FOLLOWING : 0,
    ).toBeTruthy()
    await expectNoA11yViolations(target)
  })

  it('counts a narrowed setting as managed too', async () => {
    policy.narrowed.add('analytics.enabled')
    const target = render()
    await tick()
    expect(target.textContent).toContain(SECTION_NOTE)
  })

  it('says nothing when no row in it is managed', async () => {
    const target = render()
    await tick()
    expect(target.textContent).not.toContain(SECTION_NOTE)
  })
})
