/**
 * Direct tests for `FriendlyErrorContent.svelte`: covers markdown rendering and the
 * anchor click delegate. The parent `TransferErrorDialog.friendly.test.ts` exercises
 * the same surface via the dialog wrapper; this file pins the component in isolation
 * so the coverage check is happy and so changes to the parent don't accidentally
 * regress the markdown contract.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FriendlyErrorContent from './FriendlyErrorContent.svelte'
import type { FriendlyError } from '$lib/file-explorer/types'

const openExternalUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())
const openSystemSettingsUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: (url: string) => openExternalUrl(url),
  openSystemSettingsUrl: (url: string) => openSystemSettingsUrl(url),
}))

function makeFriendly(overrides: Partial<FriendlyError> = {}): FriendlyError {
  return {
    category: 'serious',
    title: 'Whatever',
    explanation: 'Plain text explanation.',
    suggestion: 'Plain text suggestion.',
    rawDetail: 'detail',
    retryHint: false,
    ...overrides,
  }
}

function mountContent(friendly: FriendlyError) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FriendlyErrorContent, { target, props: { friendly } })
  return target
}

describe('FriendlyErrorContent', () => {
  it('renders explanation and suggestion as plain text when no markdown', async () => {
    const target = mountContent(makeFriendly({ explanation: 'Hello world.', suggestion: 'Try again.' }))
    await tick()

    expect(target.textContent).toContain('Hello world.')
    expect(target.textContent).toContain('Try again.')
  })

  it('renders **bold** as <strong>', async () => {
    const target = mountContent(makeFriendly({ explanation: 'A **bold** thing.', suggestion: 'Plain.' }))
    await tick()

    const strong = target.querySelector('strong')
    expect(strong?.textContent).toBe('bold')
  })

  it('renders bullet lists as <ul><li>', async () => {
    const target = mountContent(makeFriendly({ explanation: 'Plain.', suggestion: '- one\n- two\n- three' }))
    await tick()

    const items = target.querySelectorAll('li')
    expect(items.length).toBe(3)
    expect(items[0].textContent).toBe('one')
    expect(items[2].textContent).toBe('three')
  })

  it('routes x-apple.systempreferences: anchors through openSystemSettingsUrl', async () => {
    openSystemSettingsUrl.mockClear()
    openExternalUrl.mockClear()

    const target = mountContent(
      makeFriendly({
        suggestion: '[Open settings](x-apple.systempreferences:com.apple.preference.security?Privacy)',
      }),
    )
    await tick()

    const link = target.querySelector<HTMLAnchorElement>('a')
    expect(link).toBeTruthy()
    link?.click()

    expect(openSystemSettingsUrl).toHaveBeenCalledWith(
      'x-apple.systempreferences:com.apple.preference.security?Privacy',
    )
    expect(openExternalUrl).not.toHaveBeenCalled()
  })

  it('routes http(s) anchors through openExternalUrl', async () => {
    openSystemSettingsUrl.mockClear()
    openExternalUrl.mockClear()

    const target = mountContent(makeFriendly({ suggestion: '[docs](https://example.com)' }))
    await tick()

    const link = target.querySelector<HTMLAnchorElement>('a')
    expect(link).toBeTruthy()
    link?.click()

    expect(openExternalUrl).toHaveBeenCalledWith('https://example.com')
    expect(openSystemSettingsUrl).not.toHaveBeenCalled()
  })

  it('does not crash when click target has no anchor parent', async () => {
    const target = mountContent(makeFriendly({ explanation: 'Plain text.' }))
    await tick()

    const div = target.querySelector('.error-content') as HTMLElement
    div.click() // Click on the container itself, not on a link
    // Test passes if no exception thrown.
    expect(true).toBe(true)
  })
})
