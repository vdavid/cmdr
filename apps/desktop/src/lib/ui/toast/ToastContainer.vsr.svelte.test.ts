// @vitest-environment jsdom
/**
 * What a screen reader hears when a toast shows up, through `@guidepup/virtual-screen-reader`.
 *
 * The toast stack speaks through two live regions that are on the page before any toast is:
 * a polite one for status toasts and an assertive one for warnings and failures, side by side,
 * neither inside the other or inside the stack. A region inserted together with its first
 * message announces unreliably in WebKit, and an `alert` nested in a polite region gets two
 * politeness levels at once. It models the ARIA spec over jsdom, not VoiceOver; V9 in David's
 * guide is the listening check.
 */

import { describe, it, expect, afterEach, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { virtual } from '@guidepup/virtual-screen-reader'
import ToastContainer from './ToastContainer.svelte'
import ToastIdFixture from './toast-id-fixture.svelte'
import { addToast, clearAllToasts } from './toast-store.svelte'
import { eventually, pastPendingFrames } from '#lib/test-frames.js'
import { untilSpoken } from '#lib/test-vsr.js'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
})

afterEach(async () => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  await virtual.stop()
  document.body.innerHTML = ''
})

async function setup(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(ToastContainer, { target })
  await tick()
  await virtual.start({ container: document.body })
  return target
}

async function spoken(): Promise<string[]> {
  return await virtual.spokenPhraseLog()
}

describe('the toast stack, spoken', () => {
  it('has a polite and an assertive region before any toast, as siblings outside the stack', async () => {
    const target = await setup()

    const polite = target.querySelector('[aria-live="polite"]')
    const assertive = target.querySelector('[aria-live="assertive"]')
    expect(polite).not.toBeNull()
    expect(assertive).not.toBeNull()
    expect(polite?.contains(assertive ?? null)).toBe(false)
    expect(assertive?.contains(polite ?? null)).toBe(false)
    expect(polite?.closest('[data-toast-stack]')).toBeNull()
  })

  it('says a status toast politely', async () => {
    await setup()

    addToast('Copied 3 files.', { level: 'info' })

    await untilSpoken('polite: Copied 3 files.')
  })

  it('says a failure assertively, and only once', async () => {
    await setup()

    addToast('Couldn’t open “a.txt”.', { level: 'error' })
    await untilSpoken('assertive: Couldn’t open “a.txt”.')
    await pastPendingFrames()

    const log = await spoken()
    expect(log.filter((phrase) => phrase.includes('a.txt'))).toHaveLength(1)
  })

  it('nests no live region or alert inside the stack', async () => {
    const target = await setup()

    addToast('Saved.', { level: 'success' })
    addToast('Slow mount.', { level: 'warn' })

    const stack = target.querySelector('[data-toast-stack]')
    await eventually(() => {
      expect(stack?.querySelectorAll('[data-toast-frame]')).toHaveLength(2)
    })
    expect(stack?.querySelector('[aria-live], [role="alert"], [role="status"]')).toBeNull()
  })

  it('says two toasts raised together, not just the second', async () => {
    await setup()

    addToast('Copied 3 files.', { level: 'info' })
    addToast('Skipped 1 file.', { level: 'info' })

    await eventually(async () => {
      expect((await spoken()).join(' | ')).toMatch(/Copied 3 files\..*Skipped 1 file\./)
    })
  })

  it('says a re-raised toast again, with its new words', async () => {
    await setup()

    addToast('Indexing…', { id: 'index', level: 'info' })
    await untilSpoken('polite: Indexing…')
    addToast('Indexing done.', { id: 'index', level: 'info' })

    await untilSpoken('polite: Indexing done.')
  })

  it('says a component toast’s text without its buttons', async () => {
    await setup()

    addToast(ToastIdFixture, { id: 'fixture', props: { label: 'Moved 3 files to the trash.' } })

    await eventually(async () => {
      expect((await spoken()).join(' | ')).toContain('Moved 3 files to the trash.')
    })
    await pastPendingFrames()
    const log = (await spoken()).join(' | ')
    expect(log).not.toContain('Dismiss notification')
  })
})
