/**
 * Tier-3 a11y tests for `EmptyState.svelte`.
 *
 * The empty state surfaces a "Try…" line, three example chips (AI prompts or
 * filename patterns depending on `aiEnabled`), an index-size status line, and
 * a keyboard-shortcut tip. Covered variants: AI-on and AI-off chip sets.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import EmptyState from './EmptyState.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof EmptyState>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    aiEnabled: true,
    indexEntryCount: 10_123_456,
    onPick: () => {},
    ...overrides,
  }
}

describe('EmptyState a11y', () => {
  it('AI-on variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI-off variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps({ aiEnabled: false }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('zero-entry index has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps({ indexEntryCount: 0 }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
