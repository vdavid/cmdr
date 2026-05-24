/**
 * Behaviour tests for `CloudProviderPicker.svelte` (M3): keyboard nav (ArrowDown / Up
 * / Home / End), type-to-jump, click-to-select, and ARIA listbox semantics.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import CloudProviderPicker from './CloudProviderPicker.svelte'
import { cloudProviderPresets } from '$lib/settings'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount>; selected: string } | undefined

function mountPicker(initial: string = cloudProviderPresets[0].id) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const ctx = { value: initial }
  const instance = mount(CloudProviderPicker, {
    target,
    props: {
      value: ctx.value,
      onChange: (next: string) => {
        ctx.value = next
        // Re-mount to drive `value` reactively. Cheaper than wiring a wrapper Svelte
        // component; the picker doesn't internally cache `value`, so a re-mount keeps
        // tests simple.
        void unmount(instance)
        const newInst = mount(CloudProviderPicker, {
          target,
          props: {
            value: ctx.value,
            onChange: (n: string) => {
              ctx.value = n
            },
          },
        })
        if (mounted) {
          mounted.instance = newInst
          mounted.selected = ctx.value
        }
      },
    },
  })
  mounted = { target, instance, selected: ctx.value }
  return mounted
}

function listEl(target: HTMLElement): HTMLUListElement {
  const el = target.querySelector<HTMLUListElement>('ul[role="listbox"]')
  if (!el) throw new Error('listbox not found')
  return el
}

function options(target: HTMLElement): HTMLLIElement[] {
  return Array.from(target.querySelectorAll<HTMLLIElement>('li[role="option"]'))
}

async function settle(): Promise<void> {
  await tick()
  flushSync()
}

describe('CloudProviderPicker', () => {
  beforeEach(() => {})
  afterEach(async () => {
    if (mounted) {
      await unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
  })

  it('renders all 15 providers with listbox semantics', async () => {
    mountPicker()
    await settle()
    if (!mounted) throw new Error('not mounted')
    const opts = options(mounted.target)
    expect(opts).toHaveLength(cloudProviderPresets.length)
    expect(listEl(mounted.target).getAttribute('role')).toBe('listbox')
    expect(listEl(mounted.target).getAttribute('aria-label')).toBe('Cloud AI providers')
    expect(opts[0].getAttribute('aria-selected')).toBe('true')
  })

  it('ArrowDown moves selection to the next provider', async () => {
    mountPicker(cloudProviderPresets[0].id)
    await settle()
    if (!mounted) throw new Error('not mounted')
    const event = new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })
    listEl(mounted.target).dispatchEvent(event)
    await settle()
    expect(mounted.selected).toBe(cloudProviderPresets[1].id)
  })

  it('ArrowUp moves selection to the previous provider', async () => {
    mountPicker(cloudProviderPresets[2].id)
    await settle()
    if (!mounted) throw new Error('not mounted')
    const event = new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })
    listEl(mounted.target).dispatchEvent(event)
    await settle()
    expect(mounted.selected).toBe(cloudProviderPresets[1].id)
  })

  it('Home jumps to the first provider', async () => {
    mountPicker(cloudProviderPresets[5].id)
    await settle()
    if (!mounted) throw new Error('not mounted')
    listEl(mounted.target).dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true }))
    await settle()
    expect(mounted.selected).toBe(cloudProviderPresets[0].id)
  })

  it('End jumps to the last provider', async () => {
    mountPicker(cloudProviderPresets[0].id)
    await settle()
    if (!mounted) throw new Error('not mounted')
    listEl(mounted.target).dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true }))
    await settle()
    expect(mounted.selected).toBe(cloudProviderPresets[cloudProviderPresets.length - 1].id)
  })

  it('type-to-jump finds the matching provider (case-insensitive)', async () => {
    mountPicker(cloudProviderPresets[0].id) // OpenAI
    await settle()
    if (!mounted) throw new Error('not mounted')
    // "mi" should match "Mistral AI" (prefix on `mistral`).
    listEl(mounted.target).dispatchEvent(new KeyboardEvent('keydown', { key: 'm', bubbles: true, cancelable: true }))
    await settle()
    listEl(mounted.target).dispatchEvent(new KeyboardEvent('keydown', { key: 'i', bubbles: true, cancelable: true }))
    await settle()
    expect(mounted.selected).toBe('mistral')
  })

  it('click selects the provider', async () => {
    mountPicker(cloudProviderPresets[0].id)
    await settle()
    if (!mounted) throw new Error('not mounted')
    const groq = mounted.target.querySelector<HTMLLIElement>(`li[data-provider-id="groq"]`)
    if (!groq) throw new Error('groq option missing')
    groq.click()
    await settle()
    expect(mounted.selected).toBe('groq')
  })
})
