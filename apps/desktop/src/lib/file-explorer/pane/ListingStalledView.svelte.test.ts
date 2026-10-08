/**
 * The screen a pane shows while its folder's server or drive isn't answering. It's
 * the only thing between the user and a spinner they'd read as "this folder is
 * empty", so it has to say what's happening and offer both ways forward.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ListingStalledView from './ListingStalledView.svelte'
import type { StalledOn } from '$lib/ipc/bindings'

function mountView(stalledOn: StalledOn = 'server') {
  const onRetry = vi.fn()
  const onGoBack = vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ListingStalledView, { target, props: { folderPath: '/Volumes/nas/photos', stalledOn, onRetry, onGoBack } })
  return { target, onRetry, onGoBack }
}

async function detailFor(stalledOn: StalledOn): Promise<string | null | undefined> {
  const { target } = mountView(stalledOn)
  await tick()
  return target.querySelector('.detail')?.textContent
}

function button(target: HTMLElement, label: string): HTMLButtonElement {
  const found = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.includes(label))
  if (!found) throw new Error(`no "${label}" button`)
  return found
}

describe('ListingStalledView', () => {
  it('says the folder is still coming, and where', async () => {
    const { target } = mountView()
    await tick()

    expect(target.textContent).toContain('Still waiting for this folder')
    expect(target.textContent).toContain('/Volumes/nas/photos')
    expect(target.querySelector('[role="status"]')).not.toBeNull()
  })

  it('retries and goes back on request', async () => {
    const { target, onRetry, onGoBack } = mountView()
    await tick()

    button(target, 'Try again').click()
    button(target, 'Go back').click()

    expect(onRetry).toHaveBeenCalledOnce()
    expect(onGoBack).toHaveBeenCalledOnce()
  })

  // The wording names what the folder waits on only when the backend proved it from the mount.
  it('names the server for a network share', async () => {
    expect(await detailFor('server')).toBe(
      'The server it’s on isn’t answering. Cmdr keeps trying in the background and opens the folder as soon as it answers.',
    )
  })

  it('names the drive for a local disk', async () => {
    expect(await detailFor('drive')).toBe(
      'The drive it’s on isn’t answering. Cmdr keeps trying in the background and opens the folder as soon as it answers.',
    )
  })

  it('keeps the combined line when the mount could be either', async () => {
    expect(await detailFor('unknown')).toBe(
      'The server or drive it’s on isn’t answering. Cmdr keeps trying in the background and opens the folder as soon as it answers.',
    )
  })
})
