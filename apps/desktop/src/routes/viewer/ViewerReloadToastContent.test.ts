/**
 * The reload toast says why when a reload doesn't go through (its owner re-raises it with the
 * reason), offers a retry, and on success hands the page its refetch before closing (#153).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewerReloadToastContent from './ViewerReloadToastContent.svelte'
import { addToast, clearAllToasts, getToasts } from '#lib/ui/toast/toast-store.svelte.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

const viewerReload = vi.fn()
vi.mock('#lib/tauri-commands/index.js', () => ({
  viewerReload: (sessionId: string) => viewerReload(sessionId) as unknown,
}))

const toastId = 'viewer-file-changed-sess-1-grew'

function render(failureKey: MessageKey | null = null) {
  // Register the toast so the component's dismiss has something to remove.
  addToast('placeholder', { id: toastId, dismissal: 'persistent' })
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onReloaded = vi.fn()
  const onReloadRefused = vi.fn()
  const component = mount(ViewerReloadToastContent, {
    target,
    props: { toastId, sessionId: 'sess-1', kind: 'grew', failureKey, onReloaded, onReloadRefused },
  })
  return { target, component, onReloaded, onReloadRefused }
}

async function press(target: HTMLElement): Promise<void> {
  target.querySelector<HTMLButtonElement>('button')?.click()
  await Promise.resolve()
  await tick()
}

describe('ViewerReloadToastContent', () => {
  beforeEach(() => {
    clearAllToasts()
    viewerReload.mockReset()
  })
  afterEach(() => {
    clearAllToasts()
    document.body.innerHTML = ''
  })

  it('refetches and closes after a reload that went through', async () => {
    viewerReload.mockResolvedValue({ status: 'ok', data: null })
    const { target, component, onReloaded, onReloadRefused } = render()

    await press(target)

    expect(onReloaded).toHaveBeenCalledOnce()
    expect(onReloadRefused).not.toHaveBeenCalled()
    expect(getToasts().map((t) => t.id)).not.toContain(toastId)
    await unmount(component)
  })

  it('stays up and hands back why when the file was deleted', async () => {
    viewerReload.mockResolvedValue({ status: 'error', error: { kind: 'notFound', path: '/tmp/a.log' } })
    const { target, component, onReloaded, onReloadRefused } = render()

    await press(target)

    expect(onReloaded).not.toHaveBeenCalled()
    expect(onReloadRefused).toHaveBeenCalledWith('viewer.reloadToast.gone')
    expect(getToasts().map((t) => t.id)).toContain(toastId)
    await unmount(component)
  })

  it('hands back the slow-disk reason on a timeout', async () => {
    viewerReload.mockResolvedValue({ status: 'error', error: { kind: 'timedOut' } })
    const { target, component, onReloadRefused } = render()

    await press(target)

    expect(onReloadRefused).toHaveBeenCalledWith('viewer.sessionOp.slow')
    await unmount(component)
  })

  it('shows the reason and a retry once re-raised with one', async () => {
    const { target, component } = render('viewer.reloadToast.gone')

    expect(target.textContent).toContain(tString('viewer.reloadToast.gone'))
    expect(target.querySelector('button')?.textContent.trim()).toBe(tString('viewer.reloadToast.retry'))
    await unmount(component)
  })
})
