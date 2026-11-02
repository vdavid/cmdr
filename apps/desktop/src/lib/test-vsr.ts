/** Waiting for what `@guidepup/virtual-screen-reader` says, for the `*.vsr` tests. */

import { expect } from 'vitest'
import { virtual } from '@guidepup/virtual-screen-reader'
import { eventually } from '#lib/test-frames.js'

/** Waits for `phrase` to land anywhere in the reader's log. */
export async function untilSpoken(phrase: string): Promise<void> {
  await eventually(async () => {
    expect(await virtual.spokenPhraseLog()).toContain(phrase)
  })
}

/** Waits for `phrase` to be the last thing the reader said. */
export async function untilLastSpoken(phrase: string): Promise<void> {
  await eventually(async () => {
    expect(await virtual.lastSpokenPhrase()).toBe(phrase)
  })
}

/** Presses `key` and waits for the reader to say `phrase` last. */
export async function pressAndHear(key: string, phrase: string): Promise<void> {
  await virtual.press(key)
  await untilLastSpoken(phrase)
}
