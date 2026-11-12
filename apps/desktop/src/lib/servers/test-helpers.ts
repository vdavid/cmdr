/**
 * Shared helpers for the `SignInSheet.*.svelte.test.ts` files that mount the sheet.
 *
 * Each test file still declares its own `vi.mock` of `#lib/tauri-commands/index.js`: Vitest hoists
 * mocks per file, so they can't live here.
 */

import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInSheetRequest } from './sign-in-contract'

/** Drains the sheet's `onMount` seeding and any attempt in flight. */
export async function flush(times = 8) {
  for (let i = 0; i < times; i++) {
    await Promise.resolve()
    await tick()
  }
}

export async function renderSheet(request: SignInSheetRequest) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const done: unknown[] = []
  mount(SignInSheet, {
    target,
    props: {
      request,
      onDone: (result) => {
        done.push(result)
      },
    },
  })
  await flush()
  return { target, done }
}

export function typeInto(input: HTMLInputElement, value: string) {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

/** The sheet's own buttons live in the portalled dialog, not under `target`. */
export function buttonSaying(text: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll('button')].find((b) => b.textContent.includes(text))
  if (!match) throw new Error(`no button saying ${text}`)
  return match
}

/** The button whose whole label is `text`, for a label another button's contains. */
export function exactButton(text: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
  if (!match) throw new Error(`no button saying exactly ${text}`)
  return match
}

/** The protocol tab labelled `label`, which only the person ever moves. */
export function protocolTab(label: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find(
    (b) => b.textContent.trim() === label,
  )
  if (!match) throw new Error(`no protocol tab saying ${label}`)
  return match
}
