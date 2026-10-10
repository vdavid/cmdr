/**
 * The favorite-refusal toast's one way out: its button retires the toast BEFORE running
 * the action, so an action that raises its own toast never stacks under this one.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import FavoriteRefusalToastContent from './FavoriteRefusalToastContent.svelte'

const { dismissToast } = vi.hoisted(() => ({ dismissToast: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissToast }))

let mounted: ReturnType<typeof mount> | null = null

async function render(actionLabel: string | null, onAction: () => void): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(FavoriteRefusalToastContent, {
    target,
    props: { toastId: 'favorite-refusal', message: '“Projects” isn’t at ~/Projects anymore.', actionLabel, onAction },
  })
  await tick()
  return target
}

afterEach(() => {
  if (mounted) unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
  dismissToast.mockReset()
})

describe('FavoriteRefusalToastContent', () => {
  it('dismisses its own toast, then runs the action', async () => {
    const calls: string[] = []
    dismissToast.mockImplementation(() => calls.push('dismiss'))
    const target = await render('Remove favorite', () => calls.push('action'))

    target.querySelector('button')?.click()

    expect(dismissToast).toHaveBeenCalledWith('favorite-refusal')
    expect(calls).toEqual(['dismiss', 'action'])
  })

  it('shows only the sentence when there is no way out', async () => {
    const target = await render(null, vi.fn())

    expect(target.querySelector('button')).toBeNull()
    expect(target.textContent).toContain('isn’t at ~/Projects anymore')
  })
})
