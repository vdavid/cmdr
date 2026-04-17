/**
 * Tier 3 a11y tests for `InlineRenameEditor.svelte`.
 *
 * Input field with aria-live validation region. Tests cover ok, error,
 * and warning severities, plus the shake-animation state.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import InlineRenameEditor from './InlineRenameEditor.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const noop = () => {}

describe('InlineRenameEditor a11y', () => {
  it('default (severity=ok) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'report.md',
        severity: 'ok',
        shaking: false,
        ariaLabel: 'Rename file',
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('error severity with validation message has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'bad/name.txt',
        severity: 'error',
        shaking: false,
        ariaLabel: 'Rename file',
        ariaInvalid: true,
        validationMessage: 'Slashes are not allowed in file names',
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('warning severity has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'archive.tar',
        severity: 'warning',
        shaking: false,
        ariaLabel: 'Rename file',
        validationMessage: 'Extension changed from .zip to .tar',
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('shaking state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: '',
        severity: 'error',
        shaking: true,
        ariaLabel: 'Rename file',
        ariaInvalid: true,
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
