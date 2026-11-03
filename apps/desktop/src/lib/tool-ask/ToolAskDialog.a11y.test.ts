/** Tier-3 a11y: a question under the arming guard, through the host the app mounts. */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  answerToolAsk: () => Promise.resolve(),
  notifyDialogOpened: vi.fn(),
  notifyDialogClosed: vi.fn(),
}))

import ToolAskHost from './ToolAskHost.svelte'
import { receiveToolAsk, receiveToolAskDismissed } from './tool-ask.svelte'

describe('ToolAskDialog a11y', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  afterEach(() => {
    receiveToolAskDismissed('r1')
    flushSync()
  })

  it('has no a11y violations under the arming guard, and announces it', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToolAskHost, { target })
    receiveToolAsk({
      requestId: 'r1',
      startedBy: 'agent',
      facts: {
        kind: 'openFile',
        path: '/Users/you/run.command',
        shownPath: '/Users/you/run.command',
        name: 'run.command',
        isBundle: false,
        opener: 'Terminal',
      },
    })
    flushSync()
    await tick()

    expect(target.querySelector('[data-armed="false"]')).not.toBeNull()
    expect(target.querySelector('[role="status"]')?.textContent).toContain('Ask Cmdr started this.')
    await expectNoA11yViolations(target)
  })

  it('a setting question with its value rows has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToolAskHost, { target })
    receiveToolAsk({
      requestId: 'r1',
      startedBy: 'agent',
      facts: {
        kind: 'setSetting',
        id: 'behavior.textEditorApp',
        label: 'Edit files in',
        oldValue: { raw: 'system', name: { kind: 'systemDefault' } },
        newValue: { raw: 'com.apple.TextEdit', name: { kind: 'named', text: 'TextEdit' } },
      },
    })
    flushSync()
    await tick()

    expect(target.querySelector('dl.tool-ask-values code')?.textContent).toBe('system')
    await expectNoA11yViolations(target)
  })
})
