/**
 * Tier-3 a11y tests for `Chip.svelte`.
 *
 * Covers the filter variant (default, configured, disabled, open) and the recent variant
 * (with a leading mode badge). The chip is a single `<button>`; the filter variant carries
 * `aria-haspopup="dialog"` + `aria-expanded`, the `×` clear control is decorative (the keyboard
 * path is Backspace), so axe shouldn't flag a nested-interactive pattern.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet, type ComponentProps } from 'svelte'
import Chip from './Chip.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof Chip>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    label: 'Size',
    configured: false,
    isOpen: false,
    onActivate: () => {},
    onClear: () => {},
    ...overrides,
  }
}

async function mountAndAudit(props: Props): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Chip, { target, props })
  await tick()
  await expectNoA11yViolations(target)
  target.remove()
}

describe('Chip a11y', () => {
  it('filter default state has no a11y violations', async () => {
    await mountAndAudit(baseProps())
  })

  it('filter configured state (label + value + clear) has no a11y violations', async () => {
    await mountAndAudit(baseProps({ configured: true, value: '> 100 MB' }))
  })

  it('filter open state (aria-expanded=true) has no a11y violations', async () => {
    await mountAndAudit(baseProps({ isOpen: true }))
  })

  it('filter disabled state has no a11y violations', async () => {
    await mountAndAudit(baseProps({ disabled: true }))
  })

  it('recent variant with a leading badge has no a11y violations', async () => {
    await mountAndAudit(
      baseProps({
        variant: 'recent',
        label: '*.log',
        ariaLabel: 'Run recent filename search: *.log',
        leading: createRawSnippet(() => ({ render: () => '<span>Aa</span>' })),
      }),
    )
  })
})
