/**
 * Tier 3 a11y tests for `AppChoiceSelect.svelte`, the shell behind the Settings rows
 * that pick an app, in both states it renders: disabled at "Checking…" before a
 * usable answer, and loaded. Plus the "Edit files in" row on its own, the way
 * `sections.a11y.test.ts` audits `TerminalAppSelect`, because it carries its own
 * accessible name.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { TextEditorList } from '$lib/ipc/bindings'
import AppChoiceSelect from './AppChoiceSelect.svelte'
import TextEditorSelect from './TextEditorSelect.svelte'
import { CHOOSE_APP_VALUE } from './app-choice-options'

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: () => 'system',
  setSetting: vi.fn(),
  onSpecificSettingChange: () => () => {},
}))

const EDITORS: TextEditorList = {
  defaultAppName: 'TextEdit',
  defaultAppIcon: null,
  apps: [{ id: 'com.sublimetext.4', displayName: 'Sublime Text', icon: null }],
  chosenId: 'system',
}

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listTextEditors: vi.fn(() => Promise.resolve({ data: EDITORS, timedOut: false })),
}))

function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

/** What the shell renders for any answer here; the row modules own the real mapping. */
const ROWS = [
  { value: 'system', label: 'System default (TextEdit)' },
  { value: 'com.sublimetext.4', label: 'Sublime Text' },
  { value: CHOOSE_APP_VALUE, label: 'Choose an app…' },
]

/**
 * Mounts the bare shell over one answer, and waits for it to land. The option
 * callbacks ignore their argument: `mount` can't infer the shell's generic, so it
 * types the list as `unknown`.
 */
async function mountShell(timedOut: boolean): Promise<HTMLDivElement> {
  const target = container()
  mount(AppChoiceSelect, {
    target,
    props: {
      settingId: 'behavior.textEditorApp',
      ariaLabel: 'Edit files in',
      checkingLabel: 'Checking your apps…',
      pickerTitle: 'Choose a text editor',
      listApps: () => Promise.resolve({ data: EDITORS, timedOut }),
      itemsFor: () => ROWS,
      selectedIn: () => 'system',
    },
  })
  await tick()
  await tick()
  return target
}

// Sections share one jsdom document, and axe resolves ARIA id references
// document-wide, so each audit starts from an empty body.
afterEach(() => {
  document.body.innerHTML = ''
})

describe('AppChoiceSelect a11y', () => {
  it('disabled at "Checking…" has no a11y violations', async () => {
    const target = await mountShell(true)
    await expectNoA11yViolations(target)
  })

  it('loaded has no a11y violations', async () => {
    const target = await mountShell(false)
    await expectNoA11yViolations(target)
  })
})

describe('TextEditorSelect a11y', () => {
  it('has no a11y violations once the editor list has landed', async () => {
    const target = container()
    mount(TextEditorSelect, { target, props: { ariaLabel: 'Edit files in' } })
    await tick()
    await tick()
    await expectNoA11yViolations(target)
  })
})
